//! # drishti-gclog
//!
//! Parser for JVM unified GC logs (Java 9+).
//! Handles G1GC, ZGC (including generational), and Shenandoah log formats.
//!
//! ## Log Format
//!
//! Java 9+ unified logging uses:
//! ```text
//! [2024-01-15T10:30:00.123+0000][1.234s][info][gc] GC(0) Pause Young (Normal) ...
//! ```
//! Prefix: `[ISO-8601][uptime][level][tags] message`

pub mod g1;
pub mod parser;
pub mod shenandoah;
pub mod tailer;
pub mod zgc;

use chrono::{DateTime, Utc};
use drishti_core::model::GcAlgorithm;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GcLogError {
    #[error("Failed to parse GC log line: {0}")]
    ParseError(String),

    #[error("Unsupported GC log format")]
    UnsupportedFormat,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// A parsed unified log line (the prefix part, common to all collectors).
#[derive(Debug, Clone)]
pub struct UnifiedLogLine {
    pub timestamp: Option<DateTime<Utc>>,
    pub uptime_secs: Option<f64>,
    pub level: LogLevel,
    pub tags: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
}

/// Auto-detect which GC algorithm produced the log.
pub fn detect_algorithm(sample_lines: &[&str]) -> GcAlgorithm {
    for line in sample_lines {
        if line.contains("G1 Evacuation Pause")
            || line.contains("Pause Young (Normal)")
            || line.contains("Pause Young (Concurrent Start)")
            || line.contains("Pause Mixed")
        {
            return GcAlgorithm::G1;
        }
        // ZGC summary lines: "GC(N) [Young|Old] Garbage Collection (cause)"
        if line.contains("Garbage Collection") || line.contains("ZGC") {
            if line.contains("Young Garbage Collection")
                || line.contains("Old Garbage Collection")
                || line.contains("Minor Collection")
                || line.contains("Major Collection")
            {
                return GcAlgorithm::ZgcGenerational;
            }
            return GcAlgorithm::Zgc;
        }
        if line.contains("Shenandoah") || line.contains("Pause Init Mark") {
            return GcAlgorithm::Shenandoah;
        }
        if line.contains("Pause Full") && line.contains("PSYoungGen") {
            return GcAlgorithm::Parallel;
        }
    }
    GcAlgorithm::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_g1() {
        let lines = vec![
            "[info][gc] GC(5) Pause Young (Normal) (G1 Evacuation Pause) 45M->12M(512M) 3.456ms",
        ];
        assert_eq!(detect_algorithm(&lines), GcAlgorithm::G1);
    }

    #[test]
    fn detect_zgc_generational() {
        let lines =
            vec!["[info][gc] GC(0) Young Garbage Collection (Allocation Rate) 128M->64M 0.5ms"];
        assert_eq!(detect_algorithm(&lines), GcAlgorithm::ZgcGenerational);
    }

    #[test]
    fn detect_shenandoah() {
        let lines = vec!["[info][gc] GC(3) Pause Init Mark (process weakrefs) 0.234ms"];
        assert_eq!(detect_algorithm(&lines), GcAlgorithm::Shenandoah);
    }
}
