//! Health endpoint response parsing.
//!
//! Handles both Spring Boot 2.x (`details`) and 3.x (`components`) shapes.

use drishti_core::model::{HealthInfo, HealthStatus};
use serde::Deserialize;
use std::collections::HashMap;

/// Raw health response from `/actuator/health`.
#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(default)]
    pub components: HashMap<String, ComponentHealth>,
    /// Spring Boot 2.x compatibility — same shape, different key.
    #[serde(default)]
    pub details: HashMap<String, ComponentHealth>,
}

#[derive(Debug, Deserialize)]
pub struct ComponentHealth {
    pub status: String,
}

impl HealthResponse {
    /// Convert to the core HealthInfo model.
    pub fn to_health_info(&self) -> HealthInfo {
        let components_raw = if !self.components.is_empty() {
            &self.components
        } else {
            &self.details
        };

        let components = components_raw
            .iter()
            .map(|(k, v)| (k.clone(), parse_status(&v.status)))
            .collect();

        HealthInfo {
            status: parse_status(&self.status),
            components,
        }
    }
}

fn parse_status(s: &str) -> HealthStatus {
    match s.to_uppercase().as_str() {
        "UP" => HealthStatus::Up,
        "DOWN" => HealthStatus::Down,
        "OUT_OF_SERVICE" => HealthStatus::OutOfService,
        _ => HealthStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_boot3_health() {
        let json =
            r#"{"status":"UP","components":{"db":{"status":"UP"},"diskSpace":{"status":"UP"}}}"#;
        let resp: HealthResponse = serde_json::from_str(json).unwrap();
        let info = resp.to_health_info();
        assert_eq!(info.status, HealthStatus::Up);
        assert_eq!(info.components.len(), 2);
    }

    #[test]
    fn parse_boot2_health_with_details() {
        let json = r#"{"status":"UP","details":{"db":{"status":"UP"}}}"#;
        let resp: HealthResponse = serde_json::from_str(json).unwrap();
        let info = resp.to_health_info();
        assert_eq!(info.status, HealthStatus::Up);
        assert_eq!(info.components.len(), 1);
    }
}
