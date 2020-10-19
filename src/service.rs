use anyhow::{Context, Result};
use prost_types::Duration;
use serde::{Deserialize, Serialize};

use crate::util;

use crate::envoy_helpers::{EnvoyExport, EnvoyResource};

use crate::protobuf::envoy::config::cluster::v3::Cluster;
use crate::protobuf::envoy::config::cluster::v3::cluster::ClusterDiscoveryType;
use crate::protobuf::envoy::config::core::v3::Address;
use crate::protobuf::envoy::config::core::v3::address::Address as AddressType;
use crate::protobuf::envoy::config::core::v3::SocketAddress;
use crate::protobuf::envoy::config::core::v3::socket_address::PortSpecifier;
use crate::protobuf::envoy::config::endpoint::v3::ClusterLoadAssignment;
use crate::protobuf::envoy::config::endpoint::v3::Endpoint;
use crate::protobuf::envoy::config::endpoint::v3::LbEndpoint;
use crate::protobuf::envoy::config::endpoint::v3::LocalityLbEndpoints;
use crate::protobuf::envoy::config::endpoint::v3::lb_endpoint::HostIdentifier;
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
use crate::protobuf::envoy::extensions::filters::network::http_connection_manager::v3::HttpConnectionManager;
use crate::protobuf::envoy::extensions::filters::network::http_connection_manager::v3::HttpFilter;
use crate::protobuf::envoy::extensions::filters::http::router::v3::Router;
use crate::protobuf::envoy::extensions::filters::network::http_connection_manager::v3::http_connection_manager::RouteSpecifier;
use crate::protobuf::envoy::extensions::filters::network::http_connection_manager::v3::http_filter;

// @TODO target domain connect_timeout
// @TODO optional fields
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Service {
    pub id: u32,
    pub hosts: Vec<std::string::String>,
    pub policies: Vec<std::string::String>,
    pub target_domain: std::string::String,
}

impl Service {
    pub fn export(&self) -> Result<Vec<EnvoyExport>> {
        let mut result: Vec<EnvoyExport> = Vec::new();
        let cluster = self
            .export_clusters()
            .with_context(|| format!("failed to export cluster for service {}", self.id))?;

        result.push(EnvoyExport {
            key: format!("service::id::{}::cluster", self.id),
            config: EnvoyResource::Cluster(cluster),
        });

        // Listener entries
        let listener = self
            .export_listener()
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
        let (host, port) = util::host_port::parse(self.target_domain.as_str())?;
        let address = host.into();
        let port_specifier = port.or(Some(80)).map(PortSpecifier::PortValue);
        let socketaddress = AddressType::SocketAddress(SocketAddress {
            address,
            port_specifier,
            ..Default::default()
        });

        Ok(Cluster {
            name: self.cluster_name(),
            connect_timeout: Some(Duration {
                seconds: 1,
                nanos: 0,
            }),
            cluster_discovery_type: Some(ClusterDiscoveryType::Type(2)),
            dns_refresh_rate: Some(core::time::Duration::from_secs(60).into()),
            // lb_policy: DiscoveryType::LogicalDns(),
            load_assignment: Some(ClusterLoadAssignment {
                cluster_name: self.cluster_name(),
                endpoints: vec![LocalityLbEndpoints {
                    lb_endpoints: vec![LbEndpoint {
                        host_identifier: Some(HostIdentifier::Endpoint(Endpoint {
                            address: Some(Address {
                                address: Some(socketaddress),
                            }),
                            // hostname: self.target_domain.to_string(),
                            ..Default::default()
                        })),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        })
    }

    fn export_listener(&self) -> Result<Listener> {
        let mut filters = Vec::new();

        // @TODO no way, move this to a function or something.
        let mut buf = Vec::new();
        prost::Message::encode(
            &Router {
                ..Default::default()
            },
            &mut buf,
        )?;

        let config = prost_types::Any {
            type_url: "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router"
                .to_string(),
            value: buf,
        };

        let connection_manager = HttpConnectionManager {
            stat_prefix: "ingress_http".to_string(),
            codec_type: 0,
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                config_type: Some(http_filter::ConfigType::TypedConfig(config)),
            }],
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

        let mut buf = Vec::new();
        prost::Message::encode(&connection_manager, &mut buf)?;

        let config = prost_types::Any {
            type_url: "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager".to_string(),
            value: buf,
        };

        filters.push(Filter {
            name: "envoy.filters.network.http_connection_manager".to_string(),
            config_type: Some(ConfigType::TypedConfig(config)),
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
}
