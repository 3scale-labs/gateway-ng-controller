#![deny(clippy::all)]

use env_logger::Env;
use warp::Filter;

mod configuration;
mod envoy_cds;
mod envoy_helpers;
mod envoy_lds;
mod oidc;
mod processor;
// rustfmt stable will break down with #[path = "..."] in modules, so skip
// this module for now. See https://github.com/rust-lang/rustfmt/issues/4446.
#[rustfmt::skip]
mod protobuf;
mod service;
mod threescale_auth;
mod util;

use processor::MasterProcess;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    tokio::spawn(async move {
        let route = warp::path("static").and(warp::fs::dir("static"));
        warp::serve(route).run(([0, 0, 0, 0], 5001)).await;
    });

    let mut master_process = MasterProcess::default();
    master_process
        .start("0.0.0.0:5000".parse().unwrap())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn we_actually_run_tests() {}
}
