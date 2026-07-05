//! # drishti-jolokia
//!
//! Pure-Rust Jolokia HTTP/JSON client. Connects to a Jolokia agent and
//! performs bulk MBean reads in a single POST, returning parsed JvmSnapshot fields.

pub mod request;
pub mod response;
pub mod client;
pub mod converter;

pub use client::JolokiaClient;
pub use request::BulkRequestBuilder;
pub use converter::bulk_to_snapshot;
