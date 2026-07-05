//! Unified GC log prefix parser.
//!
//! Parses the `[timestamp][uptime][level][tags]` prefix common to all
//! Java 9+ GC logs, then dispatches to collector-specific parsers.
//!
//! Full G1/ZGC/Shenandoah event parsers will be built in Phase 3.

use crate::{GcLogError, LogLevel, UnifiedLogLine};
use regex::Regex;
use std::sync::LazyLock;

/// Regex for the unified log prefix.
/// Matches: `[2024-01-15T10:30:00.123+0000][1.234s][info][gc,phases] message`
static PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\[([^\]]*)\]\[([0-9.]+)s\]\[(\w+)\]\[([^\]]*)\]\s*(.*)"
    ).unwrap()
});

/// Parse a single unified log line into its prefix components + message.
pub fn parse_unified_line(line: &str) -> Result<UnifiedLogLine, GcLogError> {
    let line = line.trim();

    if let Some(caps) = PREFIX_RE.captures(line) {
        let timestamp_str = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let uptime_str = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let level_str = caps.get(3).map(|m| m.as_str()).unwrap_or("");
        let tags_str = caps.get(4).map(|m| m.as_str()).unwrap_or("");
        let message = caps.get(5).map(|m| m.as_str()).unwrap_or("").to_string();

        let timestamp = chrono::DateTime::parse_from_rfc3339(timestamp_str)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc));

        let uptime_secs = uptime_str.parse::<f64>().ok();

        let level = match level_str.to_lowercase().as_str() {
            "trace" => LogLevel::Trace,
            "debug" => LogLevel::Debug,
            "info" => LogLevel::Info,
            "warning" => LogLevel::Warning,
            "error" => LogLevel::Error,
            _ => LogLevel::Info,
        };

        let tags = tags_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(UnifiedLogLine {
            timestamp,
            uptime_secs,
            level,
            tags,
            message,
        })
    } else {
        Err(GcLogError::ParseError(format!(
            "Line doesn't match unified log format: {}",
            &line[..line.len().min(80)]
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_g1_pause_line() {
        let line = "[2024-01-15T10:30:00.123+0000][1.234s][info][gc] GC(5) Pause Young (Normal) (G1 Evacuation Pause) 45M->12M(512M) 3.456ms";
        let parsed = parse_unified_line(line).unwrap();
        assert_eq!(parsed.level, LogLevel::Info);
        assert_eq!(parsed.tags, vec!["gc"]);
        assert!(parsed.message.contains("Pause Young"));
        assert!((parsed.uptime_secs.unwrap() - 1.234).abs() < 0.001);
    }

    #[test]
    fn parse_gc_phases_line() {
        let line = "[2024-01-15T10:30:00.456+0000][2.000s][debug][gc,phases] GC(5)   Pre Evacuate Collection Set: 0.1ms";
        let parsed = parse_unified_line(line).unwrap();
        assert_eq!(parsed.tags, vec!["gc", "phases"]);
        assert_eq!(parsed.level, LogLevel::Debug);
    }

    #[test]
    fn reject_non_unified_line() {
        let line = "just some random text";
        assert!(parse_unified_line(line).is_err());
    }
}
