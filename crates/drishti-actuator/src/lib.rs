//! # drishti-actuator
//!
//! Spring Boot Actuator HTTP client with metric normalization.
//! Handles Boot 2.x, 3.x, and Micrometer naming differences transparently.

pub mod client;
pub mod converter;
pub mod health;
pub mod logfile;
pub mod normalize;
pub mod prometheus;
pub mod threads;

pub use client::ActuatorClient;
pub use converter::prometheus_to_snapshot;
pub use normalize::MetricRegistry;
pub use threads::parse_thread_dump;
