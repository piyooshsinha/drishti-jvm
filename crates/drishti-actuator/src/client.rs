//! Spring Boot Actuator HTTP client.

use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ActuatorError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Actuator not reachable at {url}: {reason}")]
    Unreachable { url: String, reason: String },

    #[error("Failed to parse response: {0}")]
    Parse(String),

    #[error("Endpoint not available: {endpoint}")]
    EndpointNotFound { endpoint: String },
}

/// Authentication for Actuator endpoints.
#[derive(Debug, Clone)]
pub enum ActuatorAuth {
    None,
    Basic { username: String, password: String },
    Bearer { token: String },
}

impl Default for ActuatorAuth {
    fn default() -> Self {
        Self::None
    }
}

/// HTTP client for Spring Boot Actuator endpoints.
#[derive(Debug, Clone)]
pub struct ActuatorClient {
    base_url: String,
    auth: ActuatorAuth,
    http: reqwest::Client,
}

impl ActuatorClient {
    /// Create a new client. `base_url` should be like `http://localhost:8080/actuator`.
    pub fn new(base_url: &str, auth: ActuatorAuth, timeout: Duration) -> Self {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            auth,
            http,
        }
    }

    /// Fetch the raw Prometheus exposition text from `/actuator/prometheus`.
    pub async fn scrape_prometheus_raw(&self) -> Result<String, ActuatorError> {
        let url = format!("{}/prometheus", self.base_url);
        let resp = self.get(&url).await?;
        Ok(resp)
    }

    /// Fetch health status from `/actuator/health`.
    pub async fn health_raw(&self) -> Result<String, ActuatorError> {
        let url = format!("{}/health", self.base_url);
        self.get(&url).await
    }

    /// Fetch thread dump from `/actuator/threaddump`.
    pub async fn thread_dump_raw(&self) -> Result<String, ActuatorError> {
        let url = format!("{}/threaddump", self.base_url);
        self.get(&url).await
    }

    /// Set a logger level via POST to `/actuator/loggers/{name}`.
    pub async fn set_log_level(&self, logger: &str, level: &str) -> Result<(), ActuatorError> {
        let url = format!("{}/loggers/{}", self.base_url, logger);
        let body = serde_json::json!({"configuredLevel": level});

        let resp = self
            .apply_auth(self.http.post(&url))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(ActuatorError::Parse(format!(
                "Failed to set log level: HTTP {}",
                resp.status()
            )))
        }
    }

    /// Generic GET with auth.
    async fn get(&self, url: &str) -> Result<String, ActuatorError> {
        let resp = self
            .apply_auth(self.http.get(url))
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() || e.is_timeout() {
                    ActuatorError::Unreachable {
                        url: url.to_string(),
                        reason: e.to_string(),
                    }
                } else {
                    ActuatorError::Http(e)
                }
            })?;

        if resp.status().as_u16() == 404 {
            return Err(ActuatorError::EndpointNotFound {
                endpoint: url.to_string(),
            });
        }

        let body = resp.text().await?;
        Ok(body)
    }

    fn apply_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            ActuatorAuth::None => builder,
            ActuatorAuth::Basic { username, password } => {
                builder.basic_auth(username, Some(password))
            }
            ActuatorAuth::Bearer { token } => builder.bearer_auth(token),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
