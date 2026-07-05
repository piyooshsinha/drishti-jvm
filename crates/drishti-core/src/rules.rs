//! Concrete tuning rule implementations.

use crate::anomaly::Severity;
use crate::model::*;
use crate::recommend::*;

pub struct IncreaseHeapRule;
impl TuningRule for IncreaseHeapRule {
    fn id(&self) -> &str {
        "increase_xmx"
    }
    fn name(&self) -> &str {
        "Increase max heap"
    }
    fn evaluate(&self, current: &JvmSnapshot, _history: &[JvmSnapshot]) -> Option<Recommendation> {
        let pct = current.heap.usage_pct()?;
        if pct < 70.0 {
            return None;
        }
        let current_mb = current.heap.max_mb() as i64;
        let suggested_mb = (current_mb as f64 * 1.5) as i64;
        Some(Recommendation {
            id: self.id().to_string(),
            category: Category::HeapSizing,
            severity: if pct > 85.0 {
                Severity::High
            } else {
                Severity::Warn
            },
            title: "Increase -Xmx".to_string(),
            current_state: format!(
                "Heap at {:.0}% ({:.0}M/{:.0}M)",
                pct,
                current.heap.used_mb(),
                current.heap.max_mb()
            ),
            suggestion: format!("Increase -Xmx from {}m to {}m", current_mb, suggested_mb),
            rationale: "Post-GC old gen should stay below 65% of Xmx".to_string(),
            confidence: if pct > 85.0 { 0.95 } else { 0.7 },
            jvm_flags: vec![
                format!("-Xmx{}m", suggested_mb),
                format!("-Xms{}m", suggested_mb),
            ],
        })
    }
}

pub struct SetXmsEqualsXmxRule;
impl TuningRule for SetXmsEqualsXmxRule {
    fn id(&self) -> &str {
        "set_xms_eq_xmx"
    }
    fn name(&self) -> &str {
        "Set Xms equal to Xmx"
    }
    fn evaluate(&self, current: &JvmSnapshot, _history: &[JvmSnapshot]) -> Option<Recommendation> {
        let xmx = current.jvm_info.max_heap_bytes?;
        let xms = current.jvm_info.initial_heap_bytes.unwrap_or(0);
        if xms >= xmx || xmx == 0 {
            return None;
        }
        Some(Recommendation {
            id: self.id().to_string(),
            category: Category::HeapSizing,
            severity: Severity::Info,
            title: "Set -Xms equal to -Xmx".to_string(),
            current_state: format!("Xms={}m, Xmx={}m", xms / 1048576, xmx / 1048576),
            suggestion: format!("Set -Xms{}m to match -Xmx", xmx / 1048576),
            rationale:
                "Avoids heap resize pauses in production; pre-allocates full heap at startup"
                    .to_string(),
            confidence: 0.9,
            jvm_flags: vec![
                format!("-Xms{}m", xmx / 1048576),
                format!("-Xmx{}m", xmx / 1048576),
            ],
        })
    }
}

pub struct SwitchToZgcRule;
impl TuningRule for SwitchToZgcRule {
    fn id(&self) -> &str {
        "switch_to_zgc"
    }
    fn name(&self) -> &str {
        "Consider switching to ZGC"
    }
    fn evaluate(&self, current: &JvmSnapshot, _history: &[JvmSnapshot]) -> Option<Recommendation> {
        if current.jvm_info.gc_algorithm == GcAlgorithm::Zgc
            || current.jvm_info.gc_algorithm == GcAlgorithm::ZgcGenerational
        {
            return None;
        }
        let heap_mb = current.heap.max_mb();
        let java_ver = current.jvm_info.java_major_version().unwrap_or(0);
        if heap_mb < 4096.0 || java_ver < 15 {
            return None;
        }
        // Check for high GC pause indicator
        let total_gc_time: i64 = current
            .gc_collectors
            .iter()
            .map(|c| c.collection_time_ms)
            .sum();
        let gc_count: i64 = current
            .gc_collectors
            .iter()
            .map(|c| c.collection_count)
            .sum();
        if gc_count == 0 {
            return None;
        }
        let avg_pause = total_gc_time as f64 / gc_count as f64;
        if avg_pause < 30.0 {
            return None;
        } // pauses are already low

        let mut flags = vec!["-XX:+UseZGC".to_string()];
        if java_ver >= 21 {
            flags.push("-XX:+ZGenerational".to_string());
        }
        Some(Recommendation {
            id: self.id().to_string(),
            category: Category::GcSelection,
            severity: Severity::Info,
            title: "Consider ZGC for lower pause times".to_string(),
            current_state: format!(
                "Using {:?} with {:.0}M heap, avg pause {:.1}ms",
                current.jvm_info.gc_algorithm, heap_mb, avg_pause
            ),
            suggestion: "Switch to ZGC for sub-10ms pauses on large heaps".to_string(),
            rationale: "ZGC targets <10ms pauses regardless of heap size; ideal for heaps >4GB"
                .to_string(),
            confidence: 0.7,
            jvm_flags: flags,
        })
    }
}

pub struct HikariPoolSizeRule;
impl TuningRule for HikariPoolSizeRule {
    fn id(&self) -> &str {
        "hikari_pool_size"
    }
    fn name(&self) -> &str {
        "HikariCP pool sizing"
    }
    fn evaluate(&self, current: &JvmSnapshot, history: &[JvmSnapshot]) -> Option<Recommendation> {
        let h = current.hikari.as_ref()?;
        if h.max == 0 {
            return None;
        }
        let util = h.utilization_pct();

        if util > 90.0 || h.is_saturated() {
            let cores = current.cpu.available_processors;
            let recommended = cores * 2 + 1; // HikariCP formula for SSDs
            if recommended > h.max {
                return Some(Recommendation {
                    id: self.id().to_string(),
                    category: Category::HikariCp,
                    severity: Severity::High,
                    title: "Increase HikariCP pool size".to_string(),
                    current_state: format!(
                        "Pool: active={}/{}, pending={}",
                        h.active, h.max, h.pending
                    ),
                    suggestion: format!(
                        "Increase maximumPoolSize from {} to {}",
                        h.max, recommended
                    ),
                    rationale: format!(
                        "HikariCP formula: cores({}) × 2 + 1 = {}",
                        cores, recommended
                    ),
                    confidence: 0.85,
                    jvm_flags: vec![],
                });
            }
        }
        // Check if pool is oversized
        let sustained_low = history.iter().rev().take(20).all(|s| {
            s.hikari
                .as_ref()
                .map(|h| h.utilization_pct() < 30.0)
                .unwrap_or(true)
        });
        if sustained_low && h.max > 5 && history.len() >= 20 {
            return Some(Recommendation {
                id: "hikari_pool_oversized".to_string(),
                category: Category::HikariCp,
                severity: Severity::Info,
                title: "HikariCP pool may be oversized".to_string(),
                current_state: format!(
                    "Pool utilization consistently below 30% (active={}/{})",
                    h.active, h.max
                ),
                suggestion: format!("Consider reducing maximumPoolSize from {}", h.max),
                rationale: "Excess idle connections waste database resources".to_string(),
                confidence: 0.6,
                jvm_flags: vec![],
            });
        }
        None
    }
}

pub struct TomcatThreadRule;
impl TuningRule for TomcatThreadRule {
    fn id(&self) -> &str {
        "tomcat_threads"
    }
    fn name(&self) -> &str {
        "Tomcat thread pool sizing"
    }
    fn evaluate(&self, current: &JvmSnapshot, _history: &[JvmSnapshot]) -> Option<Recommendation> {
        let t = current.tomcat.as_ref()?;
        if t.threads_max == 0 {
            return None;
        }
        let util = t.threads_busy as f64 / t.threads_max as f64 * 100.0;
        if util > 80.0 {
            return Some(Recommendation {
                id: self.id().to_string(),
                category: Category::ThreadPool,
                severity: if util > 95.0 {
                    Severity::High
                } else {
                    Severity::Warn
                },
                title: "Tomcat thread pool near capacity".to_string(),
                current_state: format!("Busy={}/{} ({:.0}%)", t.threads_busy, t.threads_max, util),
                suggestion: format!(
                    "Increase server.tomcat.threads.max (currently {})",
                    t.threads_max
                ),
                rationale: "Use Little's Law: threads ≈ arrival_rate × avg_service_time"
                    .to_string(),
                confidence: 0.8,
                jvm_flags: vec![],
            });
        }
        None
    }
}

/// Create a RecommendationEngine with all built-in rules.
pub fn default_engine() -> RecommendationEngine {
    let mut engine = RecommendationEngine::new(0.5);
    engine.register(Box::new(IncreaseHeapRule));
    engine.register(Box::new(SetXmsEqualsXmxRule));
    engine.register(Box::new(SwitchToZgcRule));
    engine.register(Box::new(HikariPoolSizeRule));
    engine.register(Box::new(TomcatThreadRule));
    engine
}
