//! Jolokia JSON response types.
//!
//! Every Jolokia response (single or bulk element) has the same envelope:
//! `{ request, value, status, timestamp, error?, stacktrace? }`

use serde::{Deserialize, Serialize};

/// A single Jolokia response envelope.
///
/// In a bulk response, you get a `Vec<JolokiaResponse>` — one per request.
/// Always check `status == 200` before accessing `value`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JolokiaResponse {
    /// HTTP-like status code (200 = OK, 404 = MBean not found, etc.)
    pub status: u32,

    /// The response payload — shape depends on the request type.
    /// For `read`: the attribute value(s) as a JSON object.
    /// For `exec`: the method return value.
    /// For `search`: an array of MBean names.
    #[serde(default)]
    pub value: serde_json::Value,

    /// Echo of the original request.
    #[serde(default)]
    pub request: serde_json::Value,

    /// Server-side Unix timestamp (seconds since epoch).
    #[serde(default)]
    pub timestamp: u64,

    /// Error message if status != 200.
    #[serde(default)]
    pub error: Option<String>,

    /// Java stack trace for the error (if available).
    #[serde(default, rename = "stacktrace")]
    pub stack_trace: Option<String>,
}

impl JolokiaResponse {
    /// Whether this response indicates success.
    pub fn is_ok(&self) -> bool {
        self.status == 200
    }

    /// Extract the value as a specific type, returning an error with context.
    pub fn parse_value<T: serde::de::DeserializeOwned>(&self) -> Result<T, JolokiaResponseError> {
        if !self.is_ok() {
            return Err(JolokiaResponseError::ServerError {
                status: self.status,
                message: self.error.clone().unwrap_or_default(),
            });
        }
        serde_json::from_value(self.value.clone()).map_err(|e| JolokiaResponseError::ParseError {
            message: e.to_string(),
            raw_value: self.value.to_string(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JolokiaResponseError {
    #[error("Jolokia server error (status {status}): {message}")]
    ServerError { status: u32, message: String },

    #[error("Failed to parse Jolokia value: {message}\nRaw: {raw_value}")]
    ParseError { message: String, raw_value: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ok_response() {
        let json = r#"{"status":200,"value":{"used":12345,"max":99999},"request":{},"timestamp":1700000000}"#;
        let resp: JolokiaResponse = serde_json::from_str(json).unwrap();
        assert!(resp.is_ok());
    }

    #[test]
    fn parse_error_response() {
        let json = r#"{"status":404,"error":"MBean not found","request":{},"timestamp":1700000000}"#;
        let resp: JolokiaResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.is_ok());
        assert_eq!(resp.error.as_deref(), Some("MBean not found"));
    }
}
