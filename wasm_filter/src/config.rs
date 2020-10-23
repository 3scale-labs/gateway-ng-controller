use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MappingRule {
    pattern: std::string::String,
    http_method: std::string::String, // @TODO this should be a enum, maybe from hyper
    metric_system_name: std::string::String,
    delta: u32,
}

impl MappingRule {
    fn matches(&self, method: std::string::String, path: std::string::String) -> bool {
        log::debug!(
            "MappingRule:Match: METHOD:: '{}', PATH:: '{}', MappingRULE: '{:?}'",
            method,
            path,
            self
        );

        if self.http_method != method {
            return false;
        }
        // @TODO should be a regexp
        if self.pattern != path {
            return false;
        }
        true
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Service {
    pub id: u32,
    pub hosts: Vec<std::string::String>,
    pub policies: Vec<std::string::String>,
    pub target_domain: std::string::String,
    pub proxy_rules: Vec<MappingRule>,
}

impl Service {
    pub fn match_mapping_rule(
        &self,
        method: std::string::String,
        path: std::string::String,
    ) -> (bool, std::string::String) {
        let mut metrics: HashMap<std::string::String, u32> = HashMap::new();
        for mapping_rule in &self.proxy_rules {
            if mapping_rule.matches(method.clone(), path.clone()) {
                log::debug!("Mapping rule matches: {:?}", mapping_rule);
                metrics.insert(mapping_rule.metric_system_name.clone(), mapping_rule.delta);
            }
        }
        if metrics.len() > 0 {
            return (true, serde_json::to_string(&metrics).unwrap());
        }
        (false, serde_json::to_string(&metrics).unwrap())
    }
}

thread_local! {
    static CONFIG: RefCell<Service> = RefCell::new(Service::default());
}

pub fn get_config() -> Service {
    let mut config = Service::default();
    CONFIG.with(|c| {
        let config_borrow = c.borrow();
        config = config_borrow.clone();
    });
    return config;
}

pub fn import_config(config: &str) -> Service {
    let service: Service = serde_json::from_str(config).unwrap();
    CONFIG.with(|c| {
        log::info!("HERE IS THE CONFIG");
        // *c.borrow_mut() = service.clone();
        match c.try_borrow_mut() {
            Err(e) => {
                log::info!("ERROR HERE@ {:?}", e);
            }
            Ok(mut r) => *r = service.clone(),
        }

        log::info!("FINISH IS THE CONFIG");
    });
    service
}
