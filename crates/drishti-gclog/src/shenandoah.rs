//! Shenandoah event parser — extracts pause and concurrent phase events.
//!
//! Shenandoah phases:
//!   GC(N) Pause Init Mark (cause) N.NNNms
//!   GC(N) Concurrent marking ... NM->NM
//!   GC(N) Pause Final Mark (cause) N.NNNms
//!   GC(N) Concurrent evacuation ... NM->NM
//!   GC(N) Pause Init Update Refs N.NNNms
//!   GC(N) Concurrent update references ... NM->NM
//!   GC(N) Pause Final Update Refs N.NNNms
//!   GC(N) Pause Full (cause) NM->NM(NM) N.NNNms  ← degenerated/full = CRITICAL

use crate::parser::parse_unified_line;
use chrono::Utc;
use drishti_core::model::{GcEvent, GcPhase};
use regex::Regex;
use std::sync::LazyLock;

// Shenandoah pause: GC(N) Pause (Init Mark|Final Mark|Init Update Refs|Final Update Refs|Full) ...
static SHEN_PAUSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"GC\((\d+)\)\s+Pause\s+(Init Mark|Final Mark|Init Update Refs|Final Update Refs|Full)\s+(?:\(([^)]*)\)\s+)?(?:(\d+)M->(\d+)M\((\d+)M\)\s+)?([0-9.]+)ms"
    ).unwrap()
});

// Shenandoah concurrent phase: GC(N) Concurrent (marking|evacuation|update references|...) NM->NM(NM) N.NNNms
static SHEN_CONCURRENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"GC\((\d+)\)\s+Concurrent\s+(marking|evacuation|update references|cleanup|precleaning|reset)\s+(?:(\d+)M->(\d+)M\((\d+)M\)\s+)?([0-9.]+)ms"
    ).unwrap()
});

pub fn parse_shenandoah_event(line: &str) -> Option<GcEvent> {
    let log_line = parse_unified_line(line).ok()?;

    // Try pause line
    if let Some(caps) = SHEN_PAUSE_RE.captures(&log_line.message) {
        let id: u64 = caps.get(1)?.as_str().parse().ok()?;
        let phase_str = caps.get(2)?.as_str();
        let cause = caps
            .get(3)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let heap_before = caps
            .get(4)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0)
            * 1024
            * 1024;
        let heap_after = caps
            .get(5)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0)
            * 1024
            * 1024;
        let capacity = caps
            .get(6)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0)
            * 1024
            * 1024;
        let pause_ms: f64 = caps.get(7)?.as_str().parse().ok()?;

        let phase = match phase_str {
            "Init Mark" => GcPhase::InitMark,
            "Final Mark" => GcPhase::FinalMark,
            "Init Update Refs" => GcPhase::InitUpdateRefs,
            "Final Update Refs" => GcPhase::FinalUpdateRefs,
            "Full" => GcPhase::DegeneratedGc, // Shenandoah Full = concurrent mode failure
            _ => GcPhase::Other(phase_str.to_string()),
        };

        return Some(GcEvent {
            id,
            collector: "Shenandoah".to_string(),
            cause,
            phase,
            heap_before_bytes: heap_before,
            heap_after_bytes: heap_after,
            heap_capacity_bytes: capacity,
            pause_ms,
            timestamp: log_line.timestamp.unwrap_or_else(Utc::now),
        });
    }

    // Try concurrent phase
    if let Some(caps) = SHEN_CONCURRENT_RE.captures(&log_line.message) {
        let id: u64 = caps.get(1)?.as_str().parse().ok()?;
        let phase_str = caps.get(2)?.as_str();
        let heap_before = caps
            .get(3)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0)
            * 1024
            * 1024;
        let heap_after = caps
            .get(4)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0)
            * 1024
            * 1024;
        let capacity = caps
            .get(5)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0)
            * 1024
            * 1024;
        let duration_ms: f64 = caps.get(6)?.as_str().parse().ok()?;

        let phase = match phase_str {
            "marking" => GcPhase::ConcurrentMark,
            "evacuation" => GcPhase::ConcurrentEvacuate,
            "update references" => GcPhase::ConcurrentUpdateRefs,
            _ => GcPhase::Other(format!("Concurrent {}", phase_str)),
        };

        return Some(GcEvent {
            id,
            collector: "Shenandoah".to_string(),
            cause: format!("Concurrent {}", phase_str),
            phase,
            heap_before_bytes: heap_before,
            heap_after_bytes: heap_after,
            heap_capacity_bytes: capacity,
            pause_ms: duration_ms, // Not a pause, but duration of concurrent phase
            timestamp: log_line.timestamp.unwrap_or_else(Utc::now),
        });
    }

    None
}

pub fn parse_shenandoah_log(text: &str) -> Vec<GcEvent> {
    text.lines().filter_map(parse_shenandoah_event).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_init_mark() {
        let line = "[2024-01-15T10:30:00.000+0000][5.0s][info][gc] GC(3) Pause Init Mark (process weakrefs) 0.234ms";
        let event = parse_shenandoah_event(line).unwrap();
        assert_eq!(event.phase, GcPhase::InitMark);
        assert!((event.pause_ms - 0.234).abs() < 0.001);
        assert_eq!(event.cause, "process weakrefs");
    }

    #[test]
    fn parse_full_gc_degenerated() {
        let line = "[2024-01-15T10:35:00.000+0000][10.0s][info][gc] GC(5) Pause Full (Allocation Failure) 480M->120M(512M) 567.890ms";
        let event = parse_shenandoah_event(line).unwrap();
        assert_eq!(event.phase, GcPhase::DegeneratedGc);
        assert_eq!(event.heap_before_bytes, 480 * 1024 * 1024);
    }

    #[test]
    fn parse_concurrent_marking() {
        let line = "[2024-01-15T10:30:00.500+0000][5.5s][info][gc] GC(3) Concurrent marking 400M->410M(512M) 12.345ms";
        let event = parse_shenandoah_event(line).unwrap();
        assert_eq!(event.phase, GcPhase::ConcurrentMark);
    }
}
