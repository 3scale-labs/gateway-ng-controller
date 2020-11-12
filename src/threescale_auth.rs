use crate::envoy_helpers::{encode, get_envoy_cluster};
use crate::protobuf::envoy::config::cluster::v3::Cluster;
use crate::protobuf::envoy::config::core::v3::async_data_source::Specifier;
use crate::protobuf::envoy::config::core::v3::http_uri::HttpUpstreamType;
use crate::protobuf::envoy::config::core::v3::AsyncDataSource;
use crate::protobuf::envoy::config::core::v3::HttpUri;
use crate::protobuf::envoy::config::core::v3::RemoteDataSource;
use crate::protobuf::envoy::extensions::filters::http::wasm::v3::Wasm;
use crate::protobuf::envoy::extensions::wasm::v3::plugin_config::Vm;
use crate::protobuf::envoy::extensions::wasm::v3::{PluginConfig, VmConfig};
use crate::service;
use anyhow::{Context, Result};
use prost_types::Duration;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Backend {
    pub cluster_name: String,
    pub url: url::Url,
    #[serde(flatten)]
    other: std::collections::HashMap<String, serde_json::Value>,
}

impl Backend {
    pub fn cluster(&self) -> Result<Cluster> {
        get_envoy_cluster(self.cluster_name.clone(), self.url.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreescaleAuth {
    path: String,
    wasm_config: WasmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WasmConfig {
    backend: Backend,
    #[serde(flatten)]
    other: std::collections::HashMap<String, serde_json::Value>,
}

impl ThreescaleAuth {
    pub fn cluster(&self) -> Result<Cluster> {
        self.wasm_config.backend.cluster()
    }

    pub fn build_wasm(&self, id: u32) -> Result<Wasm> {
        let wasm_config_s = serde_json::to_string_pretty(&self.wasm_config)?;
        get_wasm_filter(self.path.clone(), wasm_config_s.as_str(), id)
    }
}

fn get_wasm_filter(path: impl AsRef<Path>, auth_config: &str, id: u32) -> Result<Wasm> {
    let path = path.as_ref();
    let filename = path
        .file_name()
        .with_context(|| format!("failed to obtain file name of {}", path.display()))?;
    Ok(Wasm {
        config: Some(PluginConfig {
            name: format!("Service::{:?}", id),
            root_id: format!("Service::{:?}", id),
            vm: Some(Vm::VmConfig(VmConfig {
                vm_id: format!("Service::{:?}", id),
                runtime: "envoy.wasm.runtime.v8".to_string(),
                configuration: Some(prost_types::Any {
                    type_url: "type.googleapis.com/google.protobuf.StringValue".to_string(),
                    value: encode("vm config".to_string())?,
                }),
                code: Some(AsyncDataSource {
                    specifier: Some(Specifier::Remote(RemoteDataSource {
                        http_uri: Some(HttpUri {
                            uri: format!(
                                "http://control-plane-main:5001/static/{}",
                                filename
                                    .to_str()
                                    .context("invalid unicode in wasm file name")?
                            ),
                            timeout: Some(Duration {
                                seconds: 100,
                                nanos: 0,
                            }),
                            http_upstream_type: Some(HttpUpstreamType::Cluster(
                                "wasm_files".to_string(),
                            )),
                        }),
                        sha256: service::Service::get_wasm_filter_sha(path)
                            .context("could not compute SHA-256")?,
                        ..Default::default()
                    })),
                }),
                ..Default::default()
            })),
            configuration: Some(prost_types::Any {
                type_url: "type.googleapis.com/google.protobuf.StringValue".to_string(),
                value: encode(auth_config.to_string())?,
            }),
            ..Default::default()
        }),
    })
}
