use chrono::{DateTime, Utc};
use log::info;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use std::time::Duration;

mod config;
mod jwt;

const AUTH_BACKEND: &str = "httpbin";

#[no_mangle]
pub fn _start() {
    proxy_wasm::set_log_level(LogLevel::Trace);
    proxy_wasm::set_http_context(|context_id, _| -> Box<dyn HttpContext> {
        Box::new(HttpHeaders { context_id })
    });
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> { Box::new(ConfigContext) });
}

struct HttpHeaders {
    context_id: u32,
}

struct ConfigContext;

impl Context for ConfigContext {}

impl RootContext for ConfigContext {
    fn on_vm_start(&mut self, _: usize) -> bool {
        let config = self.get_configuration();

        let service = config::import_config(std::str::from_utf8(&config.unwrap()).unwrap());
        for policy in &service.policies {
            if policy.name.as_str() == "jwt" {
                let cb =
                    |context_id, _| -> Box<dyn HttpContext> { Box::new(jwt::JWT::new(context_id)) };
                proxy_wasm::set_http_context(cb);
            }
        }

        self.set_tick_period(Duration::from_secs(20));
        true
    }

    fn on_tick(&mut self) {
        let datetime: DateTime<Utc> = self.get_current_time().into();
        log::debug!("Wasm filter tick: {}", datetime);
    }
}

impl Context for HttpHeaders {
    fn on_http_call_response(&mut self, _: u32, _: usize, _: usize, _: usize) {
        let headers = self.get_http_call_response_headers();
        for (name, value) in &headers {
            if name.as_str() == ":status" && value.as_str() == "200" {
                log::info!("Access granted.");
                self.resume_http_request();
                return;
            }
        }

        self.send_http_response(403, vec![], Some(b"Access forbidden.\n"));
        return;
    }
}

impl HttpHeaders {
    fn get_method(&self) -> Option<std::string::String> {
        return self.get_http_request_header(":method");
    }

    fn get_path(&self) -> Option<std::string::String> {
        return self.get_http_request_header(":path");
    }

    fn authrep(&self, metrics: std::string::String) {
        // @TODO move this headers to a proper ones.
        self.dispatch_http_call(
            AUTH_BACKEND,
            vec![
                (":method", "GET"),
                (":path", "/headers"),
                (":authority", "httpbin.org"),
            ],
            Some(metrics.as_bytes()),
            Vec::new(),
            Duration::from_secs(5),
        )
        .unwrap();
    }
}

impl HttpContext for HttpHeaders {
    fn on_http_request_headers(&mut self, _: usize) -> Action {
        let config = config::get_config();

        let (status, metrics) =
            config.match_mapping_rule(self.get_method().unwrap(), self.get_path().unwrap());
        if !status {
            self.send_http_response(403, vec![], Some(b"Mapping rule not found\n"));
        } else {
            self.authrep(metrics);
        }
        Action::Pause
    }

    fn on_log(&mut self) {
        info!("#Request with context_id='{}' completed.", self.context_id);
    }
}
