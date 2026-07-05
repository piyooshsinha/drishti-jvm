//! G1GC event parser — extracts pause events from unified GC logs.

use crate::parser::parse_unified_line;
use chrono::Utc;
use drishti_core::model::{GcEvent, GcPhase};
use regex::Regex;
use std::sync::LazyLock;

static G1_PAUSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"GC\((\d+)\)\s+Pause\s+(Young|Mixed|Full)\s+\(([^)]+)\)(?:\s+\(([^)]+)\))?\s+(\d+)M->(\d+)M\((\d+)M\)\s+([0-9.]+)ms"
    ).unwrap()
});

/// Parse a single GC log line into a GcEvent (G1GC format).
pub fn parse_g1_event(line: &str) -> Option<GcEvent> {
    let log_line = parse_unified_line(line).ok()?;
    let caps = G1_PAUSE_RE.captures(&log_line.message)?;

    let id: u64 = caps.get(1)?.as_str().parse().ok()?;
    let phase_str = caps.get(2)?.as_str();
    let cause = caps.get(3)?.as_str().to_string();
    let heap_before: i64 = caps.get(5)?.as_str().parse::<i64>().ok()? * 1024 * 1024;
    let heap_after: i64 = caps.get(6)?.as_str().parse::<i64>().ok()? * 1024 * 1024;
    let capacity: i64 = caps.get(7)?.as_str().parse::<i64>().ok()? * 1024 * 1024;
    let pause_ms: f64 = caps.get(8)?.as_str().parse().ok()?;

    let phase = match phase_str {
        "Young" => GcPhase::YoungPause,
        "Mixed" => GcPhase::MixedPause,
        "Full" => GcPhase::FullGc,
        _ => GcPhase::Other(phase_str.to_string()),
    };

    Some(GcEvent {
        id,
        collector: "G1".to_string(),
        cause,
        phase,
        heap_before_bytes: heap_before,
        heap_after_bytes: heap_after,
        heap_capacity_bytes: capacity,
        pause_ms,
        timestamp: log_line.timestamp.unwrap_or_else(Utc::now),
    })
}

/// Parse multiple lines and extract all G1 pause events.
pub fn parse_g1_log(text: &str) -> Vec<GcEvent> {
    text.lines().filter_map(parse_g1_event).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_g1_young_pause() {
        let line = "[2024-01-15T10:30:00.123+0000][1.234s][info][gc] GC(5) Pause Young (Normal) (G1 Evacuation Pause) 45M->12M(512M) 3.456ms";
        let event = parse_g1_event(line).unwrap();
        assert_eq!(event.id, 5);
        assert_eq!(event.phase, GcPhase::YoungPause);
        assert_eq!(event.heap_before_bytes, 45 * 1024 * 1024);
        assert_eq!(event.heap_after_bytes, 12 * 1024 * 1024);
        assert!((event.pause_ms - 3.456).abs() < 0.001);
    }

    #[test]
    fn parse_g1_full_gc() {
        let line = "[2024-01-15T10:35:00.000+0000][5.000s][info][gc] GC(10) Pause Full (Allocation Failure) 500M->200M(512M) 1234.567ms";
        let event = parse_g1_event(line).unwrap();
        assert_eq!(event.phase, GcPhase::FullGc);
        assert!((event.pause_ms - 1234.567).abs() < 0.001);
    }
}
