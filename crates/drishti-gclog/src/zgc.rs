//! ZGC event parser — extracts pause events from unified GC logs.
//!
//! ZGC summary lines look like:
//!   GC(0) Garbage Collection (Allocation Rate) 3304M(20%)->384M(2%)
//! Generational ZGC (Java 21+) adds Young/Old qualifiers:
//!   GC(0) Young Garbage Collection (Allocation Rate) 512M->128M
//! Pause lines:
//!   GC(0) Pause Mark Start 0.012ms
//!   GC(0) Pause Mark End 0.008ms
//!   GC(0) Pause Relocate Start 0.005ms

use crate::parser::parse_unified_line;
use chrono::Utc;
use drishti_core::model::{GcEvent, GcPhase};
use regex::Regex;
use std::sync::LazyLock;

// ZGC collection summary: GC(N) [Young|Old]? Garbage Collection (cause) NM->NM
static ZGC_COLLECTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"GC\((\d+)\)\s+(?:(Young|Old)\s+)?Garbage Collection\s+\(([^)]+)\)\s+(\d+)M(?:\(\d+%\))?->(\d+)M"
    ).unwrap()
});

// ZGC pause: GC(N) Pause (Mark Start|Mark End|Relocate Start) N.NNNms
static ZGC_PAUSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"GC\((\d+)\)\s+Pause\s+(Mark Start|Mark End|Relocate Start)\s+([0-9.]+)ms").unwrap()
});

/// Parse a ZGC collection summary line.
pub fn parse_zgc_event(line: &str) -> Option<GcEvent> {
    let log_line = parse_unified_line(line).ok()?;

    // Try collection summary first
    if let Some(caps) = ZGC_COLLECTION_RE.captures(&log_line.message) {
        let id: u64 = caps.get(1)?.as_str().parse().ok()?;
        let generation = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let cause = caps.get(3)?.as_str().to_string();
        let heap_before: i64 = caps.get(4)?.as_str().parse::<i64>().ok()? * 1024 * 1024;
        let heap_after: i64 = caps.get(5)?.as_str().parse::<i64>().ok()? * 1024 * 1024;

        let phase = match generation {
            "Young" => GcPhase::YoungPause,
            "Old" => GcPhase::FullGc,
            _ => GcPhase::ConcurrentRelocate,
        };

        return Some(GcEvent {
            id,
            collector: format!(
                "ZGC{}",
                if !generation.is_empty() {
                    format!(" {}", generation)
                } else {
                    String::new()
                }
            ),
            cause,
            phase,
            heap_before_bytes: heap_before,
            heap_after_bytes: heap_after,
            heap_capacity_bytes: 0, // ZGC summary doesn't always show capacity
            pause_ms: 0.0,          // Pause times come from separate lines
            timestamp: log_line.timestamp.unwrap_or_else(Utc::now),
        });
    }

    // Try pause line
    if let Some(caps) = ZGC_PAUSE_RE.captures(&log_line.message) {
        let id: u64 = caps.get(1)?.as_str().parse().ok()?;
        let phase_str = caps.get(2)?.as_str();
        let pause_ms: f64 = caps.get(3)?.as_str().parse().ok()?;

        let phase = match phase_str {
            "Mark Start" => GcPhase::InitMark,
            "Mark End" => GcPhase::FinalMark,
            "Relocate Start" => GcPhase::ConcurrentRelocate,
            _ => GcPhase::Other(phase_str.to_string()),
        };

        return Some(GcEvent {
            id,
            collector: "ZGC".to_string(),
            cause: phase_str.to_string(),
            phase,
            heap_before_bytes: 0,
            heap_after_bytes: 0,
            heap_capacity_bytes: 0,
            pause_ms,
            timestamp: log_line.timestamp.unwrap_or_else(Utc::now),
        });
    }

    None
}

pub fn parse_zgc_log(text: &str) -> Vec<GcEvent> {
    text.lines().filter_map(parse_zgc_event).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_zgc_classic_collection() {
        let line = "[2024-01-15T10:30:00.000+0000][5.0s][info][gc] GC(3) Garbage Collection (Allocation Rate) 3304M(20%)->384M(2%)";
        let event = parse_zgc_event(line).unwrap();
        assert_eq!(event.id, 3);
        assert_eq!(event.heap_before_bytes, 3304 * 1024 * 1024);
        assert_eq!(event.heap_after_bytes, 384 * 1024 * 1024);
    }

    #[test]
    fn parse_zgc_gen_young() {
        let line = "[2024-01-15T10:30:00.000+0000][5.0s][info][gc] GC(0) Young Garbage Collection (Allocation Rate) 512M->128M";
        let event = parse_zgc_event(line).unwrap();
        assert_eq!(event.phase, GcPhase::YoungPause);
        assert!(event.collector.contains("Young"));
    }

    #[test]
    fn parse_zgc_pause() {
        let line = "[2024-01-15T10:30:00.000+0000][5.0s][info][gc] GC(3) Pause Mark Start 0.012ms";
        let event = parse_zgc_event(line).unwrap();
        assert_eq!(event.phase, GcPhase::InitMark);
        assert!((event.pause_ms - 0.012).abs() < 0.001);
    }
}
