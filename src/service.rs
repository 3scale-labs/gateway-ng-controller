use anyhow::{Context, Result};
use data_encoding::HEXUPPER;
use prost_types::Duration;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::envoy_helpers::{encode, get_envoy_cluster, EnvoyExport, EnvoyResource};
use crate::oidc::OIDCConfig;
use crate::threescale_auth::ThreescaleAuth;
use crate::util;

use crate::protobuf::envoy::config::core::v3::AsyncDataSource;
use crate::protobuf::envoy::config::core::v3::HttpUri;
use crate::protobuf::envoy::config::core::v3::RemoteDataSource;
use crate::protobuf::envoy::config::core::v3::async_data_source::Specifier;
use crate::protobuf::envoy::config::core::v3::http_uri::HttpUpstreamType;
use crate::protobuf::envoy::extensions::filters::http::wasm::v3::Wasm;
use crate::protobuf::envoy::extensions::wasm::v3::plugin_config::Vm;
use crate::protobuf::envoy::extensions::wasm::v3::{PluginConfig, VmConfig};
use crate::protobuf::envoy::config::cluster::v3::Cluster;
use crate::protobuf::envoy::config::core::v3::Address;
use crate::protobuf::envoy::config::core::v3::SocketAddress;
use crate::protobuf::envoy::config::core::v3::address::Address as AddressType;
use crate::protobuf::envoy::config::core::v3::socket_address::PortSpecifier;
use crate::protobuf::envoy::config::listener::v3::Filter;
use crate::protobuf::envoy::config::listener::v3::FilterChain;
use crate::protobuf::envoy::config::listener::v3::Listener;
use crate::protobuf::envoy::config::listener::v3::filter::ConfigType;
use crate::protobuf::envoy::config::route::v3::Route;
use crate::protobuf::envoy::config::route::v3::RouteAction;
use crate::protobuf::envoy::config::route::v3::RouteConfiguration;
use crate::protobuf::envoy::config::route::v3::RouteMatch;
use crate::protobuf::envoy::config::route::v3::VirtualHost;
use crate::protobuf::envoy::config::route::v3::route::Action;
use crate::protobuf::envoy::config::route::v3::route_action::ClusterSpecifier;
use crate::protobuf::envoy::config::route::v3::route_match::PathSpecifier;
use crate::protobuf::envoy::extensions::filters::http::router::v3::Router;
use crate::protobuf::envoy::extensions::filters::network::http_connection_manager::v3::HttpConnectionManager;
use crate::protobuf::envoy::extensions::filters::network::http_connection_manager::v3::HttpFilter;
use crate::protobuf::envoy::extensions::filters::network::http_connection_manager::v3::http_connection_manager::RouteSpecifier;
use crate::protobuf::envoy::extensions::filters::network::http_connection_manager::v3::http_filter;
use crate::protobuf::envoy::extensions::filters::http::jwt_authn::v3::JwtAuthentication;

const WASM_FILTER_PATH: &str = "static/filter.wasm";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MappingRules {
    pattern: std::string::String,
    http_method: std::string::String, // @TODO this should be a enum, maybe from hyper
    metric_system_name: std::string::String,
    delta: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PoliciyConfig {
    pub name: String,
    configuration: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Service {
    pub id: u32,
    pub hosts: Vec<std::string::String>,
    pub policies: Vec<PoliciyConfig>,
    pub target_domain: std::string::String,
    pub proxy_rules: Vec<MappingRules>,
    pub oidc_issuer: Option<String>,
    pub auth_config: Option<ThreescaleAuth>,
}

impl Service {
    pub fn oidc_import(&self) -> Option<Result<(JwtAuthentication, Cluster)>> {
        self.oidc_issuer.as_ref().map(|oidc_issuer| {
            let mut oidc_discovery = OIDCConfig::new(oidc_issuer.to_string());
            oidc_discovery.export(self.id)
        })
    }

    pub fn export(&self) -> Result<Vec<EnvoyExport>> {
        let mut result: Vec<EnvoyExport> = Vec::new();
        let cluster = self
            .export_clusters()
            .with_context(|| format!("failed to export cluster for service {}", self.id))?;

        result.push(EnvoyExport {
            key: format!("service::id::{}::cluster", self.id),
            config: EnvoyResource::Cluster(cluster),
        });

        let oidc_envoy_filter = match self.oidc_import() {
            Some(oidc_import) => {
                let (oidc_filter, oidc_cluster) = oidc_import?;

                result.push(EnvoyExport {
                    key: oidc_cluster.clone().name,
                    config: EnvoyResource::Cluster(oidc_cluster),
                });

                Some(HttpFilter {
                    name: "envoy.filters.http.jwt_authn".to_string(),
                    config_type: Some(http_filter::ConfigType::TypedConfig(prost_types::Any {
                        type_url: "type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication"
                            .to_string(),
                        value: encode(oidc_filter)?,
                    })),
                })
            }
            None => None,
        };

        // having a cluster is mandatory for this auth config, but it could
        // be optional - we could just extract a trait to provide a cluster(s)
        // and add them here if we wanted to make this code more generic
        if let Some(ref auth_config) = self.auth_config {
            let auth_cluster = auth_config.cluster()?;
            result.push(EnvoyExport {
                key: auth_cluster.name.clone(),
                config: EnvoyResource::Cluster(auth_cluster),
            });
        }

        // Listener entries
        let listener = self
            .export_listener(oidc_envoy_filter)
            .with_context(|| format!("failed to export listener for service {}", self.id))?;
        result.push(EnvoyExport {
            key: format!("service::id::{}::listener", self.id),
            config: EnvoyResource::Listener(listener),
        });

        Ok(result)
    }

    fn cluster_name(&self) -> std::string::String {
        return format!("Cluster::service::{}", self.id);
    }

    fn export_clusters(&self) -> Result<Cluster> {
        get_envoy_cluster(self.cluster_name(), self.target_domain.clone())
    }

    fn export_listener(&self, http_filter: Option<HttpFilter>) -> Result<Listener> {
        let mut filters = Vec::new();

        let config = prost_types::Any {
            type_url: "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router"
                .to_string(),
            value: encode(Router {
                ..Default::default()
            })?,
        };

        // WASM section, @TODO move out to a new method
        let wasm_filter = Wasm {
            config: Some(PluginConfig {
                name: format!("Service::{:?}", self.id),
                root_id: format!("Service::{:?}", self.id),
                vm: Some(Vm::VmConfig(VmConfig {
                    vm_id: format!("Service::{:?}", self.id),
                    runtime: "envoy.wasm.runtime.v8".to_string(),
                    configuration: Some(prost_types::Any {
                        type_url: "type.googleapis.com/google.protobuf.StringValue".to_string(),
                        value: encode(serde_json::to_string(&self.clone())?)?,
                    }),
                    code: Some(AsyncDataSource {
                        specifier: Some(Specifier::Remote(RemoteDataSource {
                            http_uri: Some(HttpUri {
                                uri: format!("http://control-plane-main:5001/{}", WASM_FILTER_PATH),
                                timeout: Some(Duration {
                                    seconds: 100,
                                    nanos: 0,
                                }),
                                http_upstream_type: Some(HttpUpstreamType::Cluster(
                                    "wasm_files".to_string(),
                                )),
                            }),
                            sha256: Self::get_wasm_filter_sha(WASM_FILTER_PATH)
                                .context("could not compute SHA-256")?,
                            ..Default::default()
                        })),
                    }),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        };

        let mut http_filters = Vec::new();
        if let Some(filter) = http_filter {
            http_filters.push(filter);
        }

        if let Some(ref threescale_auth) = self.auth_config {
            http_filters.push(HttpFilter {
                name: "envoy.filters.http.wasm".to_string(),
                config_type: Some(http_filter::ConfigType::TypedConfig(prost_types::Any {
                    type_url: "type.googleapis.com/envoy.extensions.filters.http.wasm.v3.Wasm"
                        .to_string(),
                    value: encode(threescale_auth.build_wasm(self.id)?)?,
                })),
            });
        }

        http_filters.push(HttpFilter {
            name: "envoy.filters.http.wasm".to_string(),
            config_type: Some(http_filter::ConfigType::TypedConfig(prost_types::Any {
                type_url: "type.googleapis.com/envoy.extensions.filters.http.wasm.v3.Wasm"
                    .to_string(),
                value: encode(wasm_filter)?,
            })),
        });

        http_filters.push(HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            config_type: Some(http_filter::ConfigType::TypedConfig(config)),
        });

        let connection_manager = HttpConnectionManager {
            stat_prefix: "ingress_http".to_string(),
            codec_type: 0,
            http_filters,
            route_specifier: Some(RouteSpecifier::RouteConfig(RouteConfiguration {
                name: format!("service_{:?}_route", self.id),
                virtual_hosts: vec![VirtualHost {
                    name: format!("service_{:?}_vhost", self.id),
                    domains: self.hosts.clone(),
                    routes: vec![Route {
                        r#match: Some(RouteMatch {
                            path_specifier: Some(PathSpecifier::Prefix("/".to_string())),
                            ..Default::default()
                        }),
                        action: Some(Action::Route(RouteAction {
                            cluster_specifier: Some(ClusterSpecifier::Cluster(self.cluster_name())),
                            ..Default::default()
                        })),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            })),
            ..Default::default()
        };

        filters.push(Filter {
            name: "envoy.filters.network.http_connection_manager".to_string(),
            config_type: Some(ConfigType::TypedConfig(
              prost_types::Any {
                type_url: "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager".to_string(),
                value: encode(connection_manager)?,
              }))
        });

        Ok(Listener {
            name: format!("service {}", self.id),
            address: Some(Address {
                address: Some(AddressType::SocketAddress(SocketAddress {
                    address: "0.0.0.0".to_string(),
                    port_specifier: Some(PortSpecifier::PortValue(80)),
                    ..Default::default()
                })),
            }),
            filter_chains: vec![FilterChain {
                filters,
                ..Default::default()
            }],
            ..Default::default()
        })
    }

    pub fn get_wasm_filter_sha(path: impl AsRef<Path>) -> Result<std::string::String> {
        let path = path.as_ref();
        let input = File::open(path)
            .with_context(|| format!("failed to open wasm filter: {}", path.display()))?;
        let reader = BufReader::new(input);
        let result = util::file_utils::sha256_digest(reader)?;
        Ok(HEXUPPER.encode(result.as_ref()).to_lowercase())
    }
}
