//! Remote log tailing via `/actuator/logfile` endpoint.
//!
//! Spring Boot's `/actuator/logfile` endpoint supports HTTP Range headers,
//! enabling "tail -f" behavior over HTTP without SSH or filesystem access.
//!
//! Technique:
//! 1. HEAD /actuator/logfile → get Content-Length (= current file size)
//! 2. GET with Range: bytes=N- → get new bytes since last read
//! 3. Parse new lines, increment offset
//! 4. Repeat on poll interval
//!
//! This is how Spring Boot Admin implements remote log tailing.

use crate::client::{ActuatorAuth, ActuatorError};
use std::time::Duration;
use tokio::sync::mpsc;

/// A chunk of new log text from the remote logfile.
#[derive(Debug, Clone)]
pub struct LogChunk {
    pub text: String,
    pub offset: u64,
    pub total_size: u64,
}

/// Remote log file tailer using HTTP Range requests.
pub struct LogFileTailer {
    base_url: String,
    auth: ActuatorAuth,
    http: reqwest::Client,
    offset: u64,
    poll_interval: Duration,
}

impl LogFileTailer {
    pub fn new(base_url: &str, auth: ActuatorAuth, poll_interval: Duration) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build reqwest client");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            auth,
            http,
            offset: 0,
            poll_interval,
        }
    }

    /// Initialize by getting the current file size (start from tail, not beginning).
    pub async fn init(&mut self) -> Result<u64, ActuatorError> {
        let url = format!("{}/logfile", self.base_url);
        let resp = self
            .apply_auth(self.http.head(&url))
            .send()
            .await
            .map_err(|e| ActuatorError::Unreachable {
                url: url.clone(),
                reason: e.to_string(),
            })?;

        if resp.status().as_u16() == 404 {
            return Err(ActuatorError::EndpointNotFound {
                endpoint: "/actuator/logfile".to_string(),
            });
        }

        let size = resp
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        // Start from near the end — grab last 4KB for initial context
        self.offset = size.saturating_sub(4096);
        Ok(size)
    }

    /// Fetch new log content since last read using Range header.
    pub async fn poll(&mut self) -> Result<Option<LogChunk>, ActuatorError> {
        let url = format!("{}/logfile", self.base_url);

        let resp = self
            .apply_auth(self.http.get(&url))
            .header("Range", format!("bytes={}-", self.offset))
            .send()
            .await
            .map_err(ActuatorError::Http)?;

        let status = resp.status().as_u16();

        match status {
            // 206 Partial Content — we got new data
            206 => {
                // Parse Content-Range: bytes N-M/total
                let total_size = resp
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.split('/').next_back())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);

                let body = resp
                    .text()
                    .await
                    .map_err(|e| ActuatorError::Parse(e.to_string()))?;

                if body.is_empty() {
                    return Ok(None);
                }

                let chunk = LogChunk {
                    text: body.clone(),
                    offset: self.offset,
                    total_size,
                };

                self.offset += body.len() as u64;
                Ok(Some(chunk))
            }
            // 200 OK — server doesn't support Range, sent everything
            200 => {
                let body = resp
                    .text()
                    .await
                    .map_err(|e| ActuatorError::Parse(e.to_string()))?;

                if body.len() as u64 <= self.offset {
                    return Ok(None); // No new content
                }

                // Extract only new content
                let new_content = if self.offset > 0 && (self.offset as usize) < body.len() {
                    body[self.offset as usize..].to_string()
                } else {
                    body.clone()
                };

                let chunk = LogChunk {
                    text: new_content,
                    offset: self.offset,
                    total_size: body.len() as u64,
                };

                self.offset = body.len() as u64;
                Ok(Some(chunk))
            }
            // 416 Range Not Satisfiable — file was probably rotated (got smaller)
            416 => {
                tracing::info!("Log file rotated (416), resetting offset");
                self.offset = 0;
                Ok(None)
            }
            404 => Err(ActuatorError::EndpointNotFound {
                endpoint: "/actuator/logfile".to_string(),
            }),
            _ => Err(ActuatorError::Parse(format!(
                "Unexpected status {} from /logfile",
                status
            ))),
        }
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

    pub fn offset(&self) -> u64 {
        self.offset
    }
}

/// Spawn a background task that tails the remote logfile and sends chunks.
pub async fn spawn_remote_log_tailer(
    base_url: String,
    auth: ActuatorAuth,
    tx: mpsc::Sender<LogChunk>,
    cancel: tokio_util::sync::CancellationToken,
) {
    let poll_interval = Duration::from_millis(1000);
    let mut tailer = LogFileTailer::new(&base_url, auth, poll_interval);

    // Initialize — get current file size
    match tailer.init().await {
        Ok(size) => {
            tracing::info!("Remote log tailer initialized, file size: {} bytes", size);
        }
        Err(e) => {
            tracing::warn!(
                "Remote log tailer init failed: {} — /actuator/logfile may not be enabled",
                e
            );
            return;
        }
    }

    let mut interval = tokio::time::interval(poll_interval);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = interval.tick() => {
                match tailer.poll().await {
                    Ok(Some(chunk)) => {
                        if tx.send(chunk).await.is_err() {
                            break; // Receiver dropped
                        }
                    }
                    Ok(None) => {} // No new data
                    Err(e) => {
                        tracing::debug!("Remote log poll error: {}", e);
                    }
                }
            }
        }
    }
}
