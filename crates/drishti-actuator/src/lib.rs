//! # drishti-actuator
//!
//! Spring Boot Actuator HTTP client with metric normalization.
//! Handles Boot 2.x, 3.x, and Micrometer naming differences transparently.

pub mod client;
pub mod health;
pub mod prometheus;
pub mod converter;
pub mod threads;
pub mod normalize;
pub mod logfile;

pub use client::ActuatorClient;
pub use converter::prometheus_to_snapshot;
pub use threads::parse_thread_dump;
pub use normalize::MetricRegistry;
