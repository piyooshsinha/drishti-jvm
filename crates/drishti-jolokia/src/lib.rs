//! # drishti-jolokia
//!
//! Pure-Rust Jolokia HTTP/JSON client. Connects to a Jolokia agent and
//! performs bulk MBean reads in a single POST, returning parsed JvmSnapshot fields.

pub mod client;
pub mod converter;
pub mod request;
pub mod response;

pub use client::JolokiaClient;
pub use converter::bulk_to_snapshot;
pub use request::BulkRequestBuilder;
