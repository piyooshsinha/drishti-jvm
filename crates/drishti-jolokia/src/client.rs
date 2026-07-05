//! Jolokia HTTP client.
//!
//! Connects to a Jolokia agent and performs bulk MBean reads,
//! returning results as `JolokiaResponse` vectors.

use crate::request::{BulkRequestBuilder, JolokiaRequest};
use crate::response::JolokiaResponse;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JolokiaError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Failed to parse Jolokia response: {0}")]
    Parse(String),

    #[error("Jolokia agent not reachable at {url}: {reason}")]
    Unreachable { url: String, reason: String },
}

/// Authentication configuration for Jolokia.
#[derive(Debug, Clone, Default)]
pub enum JolokiaAuth {
    #[default]
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
}

/// HTTP client for a single Jolokia agent.
///
/// Handles connection, authentication, bulk requests, and error recovery.
/// Designed to be held in a long-lived tokio task.
#[derive(Debug, Clone)]
pub struct JolokiaClient {
    base_url: String,
    auth: JolokiaAuth,
    http: reqwest::Client,
}

impl JolokiaClient {
    /// Create a new client pointing at a Jolokia agent.
    ///
    /// `base_url` should be like `http://localhost:8778/jolokia`
    pub fn new(base_url: &str, auth: JolokiaAuth, timeout: Duration) -> Self {
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

    /// Health check — hit the Jolokia version endpoint.
    pub async fn health_check(&self) -> Result<bool, JolokiaError> {
        let url = format!("{}/version", self.base_url);
        let resp = self.apply_auth(self.http.get(&url)).send().await?;
        Ok(resp.status().is_success())
    }

    /// Execute a bulk request and return raw responses.
    ///
    /// This is the core method — one HTTP POST, multiple MBean reads.
    pub async fn bulk_read(
        &self,
        requests: &[JolokiaRequest],
    ) -> Result<Vec<JolokiaResponse>, JolokiaError> {
        let resp = self
            .apply_auth(self.http.post(&self.base_url))
            .header("Content-Type", "application/json")
            .json(requests)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() || e.is_timeout() {
                    JolokiaError::Unreachable {
                        url: self.base_url.clone(),
                        reason: e.to_string(),
                    }
                } else {
                    JolokiaError::Http(e)
                }
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(JolokiaError::Parse(format!(
                "HTTP {}: {}",
                status,
                &body[..body.len().min(200)]
            )));
        }

        let responses: Vec<JolokiaResponse> = resp.json().await?;
        Ok(responses)
    }

    /// Execute the standard bulk request that captures core JVM state.
    pub async fn fetch_standard(&self) -> Result<Vec<JolokiaResponse>, JolokiaError> {
        let requests = BulkRequestBuilder::standard();
        self.bulk_read(&requests).await
    }

    /// Apply authentication to a request builder.
    fn apply_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            JolokiaAuth::None => builder,
            JolokiaAuth::Basic { username, password } => {
                builder.basic_auth(username, Some(password))
            }
            JolokiaAuth::Bearer { token } => builder.bearer_auth(token),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_trims_trailing_slash() {
        let client = JolokiaClient::new(
            "http://localhost:8778/jolokia/",
            JolokiaAuth::None,
            Duration::from_secs(5),
        );
        assert_eq!(client.base_url(), "http://localhost:8778/jolokia");
    }
}
