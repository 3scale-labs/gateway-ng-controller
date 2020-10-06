use prost_types::Duration;
use serde::{Deserialize, Serialize};

use crate::envoy_helpers::{EnvoyExport, EnvoyResource};

use crate::protobuf::envoy::config::cluster::v3::Cluster;
use crate::protobuf::envoy::config::cluster::v3::cluster::ClusterDiscoveryType;
use crate::protobuf::envoy::config::core::v3::Address;
use crate::protobuf::envoy::config::core::v3::SocketAddress;
use crate::protobuf::envoy::config::core::v3::socket_address::PortSpecifier::PortValue;
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
use crate::protobuf::envoy::extensions::filters::network::http_connection_manager::v3::http_connection_manager::RouteSpecifier;

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
    pub fn export(&self) -> Vec<EnvoyExport> {
        let mut result: Vec<EnvoyExport> = Vec::new();
        let cluster = self.export_clusters();

        result.push(EnvoyExport {
            key: format!("service::id::{}::cluster", self.id),
            config: EnvoyResource::Cluster(cluster),
        });

        // Listener entries
        let listener = self.export_listener();
        result.push(EnvoyExport {
            key: format!("service::id::{}::listener", self.id),
            config: EnvoyResource::Listener(listener),
        });

        result
    }
    fn cluster_name(&self) -> std::string::String {
        return format!("Cluster::service::{}", self.id);
    }

    fn export_clusters(&self) -> Cluster {
        let socketaddress =
            crate::protobuf::envoy::config::core::v3::address::Address::SocketAddress(
                SocketAddress {
                    address: self.target_domain.to_string(),
                    // resolver_name: self.target_domain.to_string(),
                    port_specifier: Some(crate::protobuf::envoy::config::core::v3::socket_address::PortSpecifier::PortValue(10000)),
                    ..Default::default()
                },
            );
        Cluster {
            name: self.cluster_name(),
            connect_timeout: Some(Duration {
                seconds: 1,
                nanos: 0,
            }),
            cluster_discovery_type: Some(ClusterDiscoveryType::Type(2)),
            // lb_policy: DiscoveryType::LogicalDns(),
            load_assignment: Some(ClusterLoadAssignment {
                cluster_name: self.cluster_name(),
                endpoints: vec![LocalityLbEndpoints {
                    lb_endpoints: vec![LbEndpoint {
                        host_identifier: Some(HostIdentifier::Endpoint(Endpoint {
                            address: Some(Address {
                                address: Some(socketaddress),
                            }),
                            hostname: self.target_domain.to_string(),
                            ..Default::default()
                        })),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn export_listener(&self) -> Listener {
        let mut filters = Vec::new();

        let connection_manager = HttpConnectionManager {
            stat_prefix: "ingress_http".to_string(),
            codec_type: 0,
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
        prost::Message::encode(&connection_manager, &mut buf).unwrap();

        let config = prost_types::Any {
            type_url: "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager".to_string(),
            value: buf,
        };

        filters.push(Filter {
            name: "envoy.http_connection_manager".to_string(),
            config_type: Some(ConfigType::TypedConfig(config)),
        });

        Listener {
            name: format!("service {}", self.id),
            address: Some(Address {
                address: Some(
                    crate::protobuf::envoy::config::core::v3::address::Address::SocketAddress(
                        SocketAddress {
                            address: "0.0.0.0".to_string(),
                            port_specifier: Some(PortValue(80)),
                            ..Default::default()
                        },
                    ),
                ),
            }),
            filter_chains: vec![FilterChain {
                filters,
                ..Default::default()
            }],
            ..Default::default()
        }
    }
}
