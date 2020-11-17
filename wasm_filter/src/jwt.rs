use proxy_wasm::traits::*;
use proxy_wasm::types::*;

use prost::Message;

#[derive(Debug, Default)]
pub struct Rules {
    path: String,
    claim: String,       // Move to liquid
    claim_value: String, // Move to liquid
    allow: bool,
}
impl Rules {
    pub fn matches(&self, path: String, jwt_claim_value: &str) -> bool {
        if self.path != path {
            log::debug!(
                "PATH failed match request_path='{}' with path='{}'",
                self.path,
                path
            );
            return false;
        }
        if self.claim_value.as_str() != jwt_claim_value {
            log::debug!(
                "Matches claim_value='{}' with jwt_value='{}' failed",
                self.claim_value,
                jwt_claim_value,
            );
            return false;
        }

        true
    }
}

#[derive(Debug, Default)]
pub struct JWTConfig {
    pub rules: Vec<Rules>,
}

#[derive(Debug, Default)]
pub struct JWT {
    pub context_id: u32,
    pub config: JWTConfig,
}

impl JWT {
    pub fn new(context_id: u32) -> JWT {
        JWT {
            context_id,
            config: JWTConfig {
                ..Default::default()
            },
        }
    }

    pub fn config(&mut self) {
        self.config.rules = Vec::new();
        self.config.rules.push(Rules {
            path: "/headers".to_string(),
            claim: "name".to_string(),
            claim_value: "Jane Smith".to_string(),
            allow: false,
        });
    }
    pub fn get_jwt_claim(&self, claim: &str) -> Result<String, anyhow::Error> {
        let key = vec![
            "metadata",
            "filter_metadata",
            "envoy.filters.http.jwt_authn",
            "jwt_payload",
            claim,
        ];

        let data = self.get_property(key);
        if data.is_none() {
            return Err(anyhow::Error::msg("Failed to get JWT payload"));
        }
        let tmp = data.clone().unwrap();

        let ret = std::str::from_utf8(tmp.as_slice());
        if ret.is_err() {
            return Err(anyhow::Error::msg("Failed to get decode JWT payload"));
        }
        Ok(ret.unwrap().to_string())
    }

    pub fn get_jwt_token(&self) -> Result<Vec<u8>, anyhow::Error> {
        let data = self.get_property(vec![
            "metadata",
            "filter_metadata",
            "envoy.filters.http.jwt_authn",
            "jwt_payload",
        ]);

        if data.is_none() {
            return Err(anyhow::Error::msg("Failed to get JWT payload"));
        }
        log::info!("Bytes --> {:?}", data.clone().unwrap().as_slice());
        log::info!(
            "STR --> {:?}",
            std::str::from_utf8(data.clone().unwrap().as_slice())
        );
        let msg: prost_types::Struct =
            Message::decode_length_delimited(data.clone().unwrap().as_slice())
                .expect("cannot decode message");
        log::info!("MSG--> {:?}", msg);
        return Ok(data.unwrap());
    }

    fn get_path(&self) -> Option<std::string::String> {
        return self.get_http_request_header(":path");
    }
}

impl Context for JWT {}

impl HttpContext for JWT {
    fn on_http_request_headers(&mut self, _: usize) -> Action {
        // @TODO to be removed until HTTP_CONTEXT can have metadata attached
        self.config();

        let jwt_token = self.get_jwt_token();
        if jwt_token.is_err() {
            log::warn!("Error on JWT auth: '{:?}'", jwt_token);
            self.send_http_response(403, vec![], Some(b"Access forbidden.\n"));
            return Action::Pause;
        }

        let mut result = false;

        for rule in &self.config.rules {
            let claim_value = self.get_jwt_claim(rule.claim.as_str());
            if claim_value.is_err() {
                log::info!("Cannot retrieve {}", rule.claim.as_str());
                continue;
            }

            if rule.matches(self.get_path().unwrap(), claim_value.unwrap().as_str()) {
                result = true;
            }
        }

        if result == true {
            return Action::Continue;
        }

        self.send_http_response(403, vec![], Some(b"Access forbidden.\n"));
        return Action::Pause;
    }
}
