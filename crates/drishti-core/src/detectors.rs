//! Concrete anomaly detector implementations.

use crate::anomaly::*;
use crate::model::*;
use chrono::Utc;

// ═══════════════════════════════════════════════════════════════
// Memory Leak Detector
// ═══════════════════════════════════════════════════════════════

pub struct MemoryLeakDetector;

impl AnomalyDetector for MemoryLeakDetector {
    fn name(&self) -> &str { "memory_leak" }

    fn evaluate(&self, current: &JvmSnapshot, history: &[JvmSnapshot]) -> Vec<Alert> {
        if history.len() < 10 { return vec![]; }
        let max_heap = current.heap.max;
        if max_heap <= 0 { return vec![]; }

        // Check if heap usage is monotonically trending up
        let recent = &history[history.len().saturating_sub(30)..];
        let n = recent.len() as f64;
        if n < 5.0 { return vec![]; }

        // Simple linear regression on heap used
        let (mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0);
        for (i, snap) in recent.iter().enumerate() {
            let x = i as f64;
            let y = snap.heap.used as f64;
            sx += x; sy += y; sxx += x * x; sxy += x * y;
        }
        let denom = n * sxx - sx * sx;
        if denom.abs() < f64::EPSILON { return vec![]; }
        let slope = (n * sxy - sx * sy) / denom;
        let intercept = (sy - slope * sx) / n;

        // R²
        let mean_y = sy / n;
        let ss_tot: f64 = recent.iter().enumerate()
            .map(|(_i, s)| (s.heap.used as f64 - mean_y).powi(2)).sum();
        let ss_res: f64 = recent.iter().enumerate()
            .map(|(i, s)| { let p = slope * i as f64 + intercept; (s.heap.used as f64 - p).powi(2) }).sum();
        let r_sq = if ss_tot > 0.0 { 1.0 - ss_res / ss_tot } else { 0.0 };

        // slope is bytes per sample interval; estimate bytes/hour
        let slope_pct_per_sample = slope / max_heap as f64 * 100.0;
        let usage_pct = current.heap.used as f64 / max_heap as f64 * 100.0;

        let mut alerts = vec![];
        if r_sq > 0.7 && slope_pct_per_sample > 0.1 && usage_pct > 60.0 {
            let severity = if slope_pct_per_sample > 0.5 && usage_pct > 80.0 {
                Severity::Critical
            } else if usage_pct > 70.0 {
                Severity::High
            } else {
                Severity::Warn
            };
            alerts.push(Alert {
                id: "mem_leak_detected".to_string(),
                severity,
                title: "Possible memory leak detected".to_string(),
                detail: format!("Heap trending upward at {:.1}%/sample (R²={:.2}), currently at {:.0}% ({:.0}M/{:.0}M)",
                    slope_pct_per_sample, r_sq, usage_pct, current.heap.used_mb(), current.heap.max_mb()),
                evidence_tab: EvidenceTab::Memory,
                first_seen: Utc::now(),
                suppressed_until: None,
                confidence: r_sq,
            });
        }
        alerts
    }
}

// ═══════════════════════════════════════════════════════════════
// GC Pressure Detector
// ═══════════════════════════════════════════════════════════════

pub struct GcPressureDetector;

impl AnomalyDetector for GcPressureDetector {
    fn name(&self) -> &str { "gc_pressure" }

    fn evaluate(&self, current: &JvmSnapshot, history: &[JvmSnapshot]) -> Vec<Alert> {
        let mut alerts = vec![];

        // Calculate GC throughput from recent history
        if history.len() >= 2 {
            let first = history.first().unwrap();
            let total_gc_ms_now: i64 = current.gc_collectors.iter().map(|c| c.collection_time_ms).sum();
            let total_gc_ms_first: i64 = first.gc_collectors.iter().map(|c| c.collection_time_ms).sum();
            let gc_delta = (total_gc_ms_now - total_gc_ms_first) as f64;
            let uptime_delta = (current.jvm_info.uptime_ms - first.jvm_info.uptime_ms) as f64;

            if uptime_delta > 0.0 {
                let throughput = 1.0 - (gc_delta / uptime_delta);
                if throughput < 0.95 {
                    let severity = if throughput < 0.85 { Severity::Critical }
                        else if throughput < 0.90 { Severity::High }
                        else { Severity::Warn };
                    alerts.push(Alert {
                        id: "gc_throughput_low".to_string(),
                        severity,
                        title: "GC throughput below target".to_string(),
                        detail: format!("GC throughput: {:.1}% (target: >95%). GC consumed {:.0}ms in {:.0}ms window",
                            throughput * 100.0, gc_delta, uptime_delta),
                        evidence_tab: EvidenceTab::Memory,
                        first_seen: Utc::now(), suppressed_until: None,
                        confidence: 0.9,
                    });
                }
            }
        }

        // Check for full GC frequency
        let full_gc_count: i64 = current.gc_collectors.iter()
            .filter(|c| c.name.to_lowercase().contains("old") || c.name.to_lowercase().contains("full"))
            .map(|c| c.collection_count)
            .sum();
        if full_gc_count > 0 && current.jvm_info.uptime_ms > 60000 {
            let full_gc_per_min = full_gc_count as f64 / (current.jvm_info.uptime_ms as f64 / 60000.0);
            if full_gc_per_min > 1.0 {
                alerts.push(Alert {
                    id: "full_gc_frequent".to_string(),
                    severity: Severity::High,
                    title: "Frequent Full GC detected".to_string(),
                    detail: format!("{:.1} Full GCs per minute ({} total)", full_gc_per_min, full_gc_count),
                    evidence_tab: EvidenceTab::Memory,
                    first_seen: Utc::now(), suppressed_until: None,
                    confidence: 0.95,
                });
            }
        }

        alerts
    }
}

// ═══════════════════════════════════════════════════════════════
// Deadlock Detector
// ═══════════════════════════════════════════════════════════════

pub struct DeadlockDetector;

impl AnomalyDetector for DeadlockDetector {
    fn name(&self) -> &str { "deadlock" }

    fn evaluate(&self, current: &JvmSnapshot, _history: &[JvmSnapshot]) -> Vec<Alert> {
        if current.deadlocks.is_empty() { return vec![]; }
        let total_threads: usize = current.deadlocks.iter().map(|d| d.thread_ids.len()).sum();
        vec![Alert {
            id: "deadlock_detected".to_string(),
            severity: Severity::Critical,
            title: format!("Thread deadlock detected ({} threads)", total_threads),
            detail: format!("Deadlocked thread IDs: {:?}", current.deadlocks.iter().flat_map(|d| &d.thread_ids).collect::<Vec<_>>()),
            evidence_tab: EvidenceTab::Threads,
            first_seen: Utc::now(), suppressed_until: None,
            confidence: 1.0,
        }]
    }
}

// ═══════════════════════════════════════════════════════════════
// Connection Pool Exhaustion Detector
// ═══════════════════════════════════════════════════════════════

pub struct PoolExhaustionDetector;

impl AnomalyDetector for PoolExhaustionDetector {
    fn name(&self) -> &str { "pool_exhaustion" }

    fn evaluate(&self, current: &JvmSnapshot, history: &[JvmSnapshot]) -> Vec<Alert> {
        let mut alerts = vec![];
        if let Some(ref h) = current.hikari {
            if h.is_saturated() {
                alerts.push(Alert {
                    id: "hikari_saturated".to_string(),
                    severity: Severity::High,
                    title: "HikariCP pool saturated".to_string(),
                    detail: format!("Active={}/{}, Pending={}, Timeouts={}",
                        h.active, h.max, h.pending, h.timeout_count),
                    evidence_tab: EvidenceTab::Db,
                    first_seen: Utc::now(), suppressed_until: None,
                    confidence: 0.95,
                });
            } else if h.pending > 0 {
                // Check if pending has been sustained
                let sustained = history.iter().rev().take(5)
                    .all(|s| s.hikari.as_ref().map(|h| h.pending > 0).unwrap_or(false));
                if sustained {
                    alerts.push(Alert {
                        id: "hikari_pending_sustained".to_string(),
                        severity: Severity::Warn,
                        title: "Sustained pending connection requests".to_string(),
                        detail: format!("Active={}/{}, Pending={}", h.active, h.max, h.pending),
                        evidence_tab: EvidenceTab::Db,
                        first_seen: Utc::now(), suppressed_until: None,
                        confidence: 0.8,
                    });
                }
            }
            if h.timeout_count > 0 {
                let prev_timeouts = history.last()
                    .and_then(|s| s.hikari.as_ref())
                    .map(|h| h.timeout_count).unwrap_or(0);
                if h.timeout_count > prev_timeouts {
                    alerts.push(Alert {
                        id: "hikari_timeout".to_string(),
                        severity: Severity::Critical,
                        title: "Connection pool timeout detected".to_string(),
                        detail: format!("Timeout count increased: {} → {}", prev_timeouts, h.timeout_count),
                        evidence_tab: EvidenceTab::Db,
                        first_seen: Utc::now(), suppressed_until: None,
                        confidence: 1.0,
                    });
                }
            }
        }
        alerts
    }
}

// ═══════════════════════════════════════════════════════════════
// High Heap Usage Detector
// ═══════════════════════════════════════════════════════════════

pub struct HighHeapDetector;

impl AnomalyDetector for HighHeapDetector {
    fn name(&self) -> &str { "high_heap" }

    fn evaluate(&self, current: &JvmSnapshot, _history: &[JvmSnapshot]) -> Vec<Alert> {
        let pct = current.heap.usage_pct().unwrap_or(0.0);
        if pct > 90.0 {
            vec![Alert {
                id: "heap_critical".to_string(),
                severity: Severity::Critical,
                title: format!("Heap usage critical: {:.0}%", pct),
                detail: format!("{:.0}M / {:.0}M used", current.heap.used_mb(), current.heap.max_mb()),
                evidence_tab: EvidenceTab::Memory,
                first_seen: Utc::now(), suppressed_until: None,
                confidence: 1.0,
            }]
        } else if pct > 80.0 {
            vec![Alert {
                id: "heap_high".to_string(),
                severity: Severity::Warn,
                title: format!("Heap usage high: {:.0}%", pct),
                detail: format!("{:.0}M / {:.0}M used", current.heap.used_mb(), current.heap.max_mb()),
                evidence_tab: EvidenceTab::Memory,
                first_seen: Utc::now(), suppressed_until: None,
                confidence: 1.0,
            }]
        } else {
            vec![]
        }
    }
}

/// Create an AnomalyEngine with all built-in detectors.
pub fn default_engine() -> AnomalyEngine {
    let mut engine = AnomalyEngine::new();
    engine.register(Box::new(MemoryLeakDetector));
    engine.register(Box::new(GcPressureDetector));
    engine.register(Box::new(DeadlockDetector));
    engine.register(Box::new(PoolExhaustionDetector));
    engine.register(Box::new(HighHeapDetector));
    engine
}
