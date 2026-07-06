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
            config_changes: vec![],
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
            config_changes: vec![],
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
            config_changes: vec![],
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
                    config_changes: vec![format!(
                        "spring.datasource.hikari.maximum-pool-size={}",
                        recommended
                    )],
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
                config_changes: vec![format!(
                    "spring.datasource.hikari.maximum-pool-size={}",
                    (h.max / 2).max(5)
                )],
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
                config_changes: vec![format!(
                    "server.tomcat.threads.max={}",
                    (t.threads_max * 2).max(200)
                )],
            });
        }
        None
    }
}

// ═══════════════════════════════════════════════════════════════
// Application-level rules (executors, caches, HTTP, logging)
// ═══════════════════════════════════════════════════════════════

pub struct ExecutorBacklogRule;
impl TuningRule for ExecutorBacklogRule {
    fn id(&self) -> &str {
        "executor_backlog"
    }
    fn name(&self) -> &str {
        "Task executor backlog"
    }
    fn evaluate(&self, current: &JvmSnapshot, history: &[JvmSnapshot]) -> Option<Recommendation> {
        let e = current
            .executors
            .iter()
            .find(|e| e.is_saturated() && e.queued > 0)?;
        // Confidence grows with how consistently the queue has been non-empty
        let backlogged = history
            .iter()
            .rev()
            .take(10)
            .filter(|s| s.executors.iter().any(|x| x.name == e.name && x.queued > 0))
            .count();
        let confidence = 0.5 + 0.05 * backlogged as f64;
        let suggested = (e.max_size.max(e.pool_size) * 2).max(4);
        Some(Recommendation {
            id: self.id().to_string(),
            category: Category::Executor,
            severity: if e.queued > e.pool_size * 2 {
                Severity::High
            } else {
                Severity::Warn
            },
            title: format!("Executor '{}' has a task backlog", e.name),
            current_state: format!(
                "active={}/{} queued={} (queue backlog in {}/10 recent samples)",
                e.active, e.pool_size, e.queued, backlogged
            ),
            suggestion: format!(
                "Raise max pool size from {} to {} or add backpressure upstream",
                e.max_size, suggested
            ),
            rationale: "All pool threads busy with tasks queueing — latency grows with the queue"
                .to_string(),
            confidence,
            jvm_flags: vec![],
            config_changes: vec![format!("spring.task.execution.pool.max-size={}", suggested)],
        })
    }
}

pub struct ExecutorOversizedRule;
impl TuningRule for ExecutorOversizedRule {
    fn id(&self) -> &str {
        "executor_oversized"
    }
    fn name(&self) -> &str {
        "Task executor oversized"
    }
    fn evaluate(&self, current: &JvmSnapshot, history: &[JvmSnapshot]) -> Option<Recommendation> {
        if history.len() < 20 {
            return None;
        }
        let e = current
            .executors
            .iter()
            .find(|e| e.core_size >= 8 && e.completed_total > 100)?;
        let peak_active = history
            .iter()
            .flat_map(|s| s.executors.iter().filter(|x| x.name == e.name))
            .map(|x| x.active)
            .max()
            .unwrap_or(0);
        if peak_active * 4 > e.core_size {
            return None;
        }
        let suggested = (peak_active * 2).max(2);
        Some(Recommendation {
            id: self.id().to_string(),
            category: Category::Executor,
            severity: Severity::Info,
            title: format!("Executor '{}' looks oversized", e.name),
            current_state: format!(
                "core={} but peak active over {} samples is {}",
                e.core_size,
                history.len(),
                peak_active
            ),
            suggestion: format!(
                "Reduce core pool size from {} to ~{}",
                e.core_size, suggested
            ),
            rationale: "Idle pool threads consume stack memory and scheduler overhead".to_string(),
            confidence: 0.6,
            jvm_flags: vec![],
            config_changes: vec![format!(
                "spring.task.execution.pool.core-size={}",
                suggested
            )],
        })
    }
}

pub struct CacheHitRatioRule;
impl TuningRule for CacheHitRatioRule {
    fn id(&self) -> &str {
        "cache_hit_ratio"
    }
    fn name(&self) -> &str {
        "Low cache hit ratio"
    }
    fn evaluate(&self, current: &JvmSnapshot, _history: &[JvmSnapshot]) -> Option<Recommendation> {
        let c = current
            .caches
            .iter()
            .filter(|c| c.gets() > 1000)
            .find(|c| c.hit_ratio().map(|r| r < 0.5).unwrap_or(false))?;
        let ratio = c.hit_ratio().unwrap_or(0.0);
        Some(Recommendation {
            id: self.id().to_string(),
            category: Category::Cache,
            severity: if ratio < 0.2 {
                Severity::Warn
            } else {
                Severity::Info
            },
            title: format!("Cache '{}' hit ratio is {:.0}%", c.name, ratio * 100.0),
            current_state: format!(
                "{} gets: {} hits / {} misses, {} evictions, size {}",
                c.gets(),
                c.hits,
                c.misses,
                c.evictions,
                c.size
            ),
            suggestion: if c.evictions > c.hits {
                "Cache is thrashing — increase maximumSize or reconsider what is cached".to_string()
            } else {
                "Most lookups miss — check key design and TTL, or drop the cache".to_string()
            },
            rationale: "A cache below ~50% hit ratio often costs more than it saves".to_string(),
            confidence: 0.7,
            jvm_flags: vec![],
            config_changes: vec![format!(
                "spring.cache.caffeine.spec=maximumSize={},expireAfterWrite=10m",
                (c.size * 2).max(1000)
            )],
        })
    }
}

pub struct HikariSlowAcquireRule;
impl TuningRule for HikariSlowAcquireRule {
    fn id(&self) -> &str {
        "hikari_slow_acquire"
    }
    fn name(&self) -> &str {
        "Slow connection acquisition"
    }
    fn evaluate(&self, current: &JvmSnapshot, _history: &[JvmSnapshot]) -> Option<Recommendation> {
        let h = current.hikari.as_ref()?;
        // Pending waiters while the pool is NOT fully used points at the
        // database (or network), not the pool size.
        if h.pending == 0 || h.max == 0 || h.utilization_pct() > 80.0 {
            return None;
        }
        Some(Recommendation {
            id: self.id().to_string(),
            category: Category::HikariCp,
            severity: Severity::Warn,
            title: "Threads wait for connections while the pool has headroom".to_string(),
            current_state: format!(
                "pending={} with active={}/{} ({:.0}% utilization)",
                h.pending,
                h.active,
                h.max,
                h.utilization_pct()
            ),
            suggestion:
                "The bottleneck is the database side (slow connect / auth / network), not pool \
                 size — check DB load and connection latency before growing the pool"
                    .to_string(),
            rationale: "Growing a pool that isn't saturated only adds idle connections".to_string(),
            confidence: 0.65,
            jvm_flags: vec![],
            config_changes: vec![
                "spring.datasource.hikari.connection-timeout=10000".to_string(),
                "spring.datasource.hikari.keepalive-time=300000".to_string(),
            ],
        })
    }
}

pub struct TomcatConnectionLimitRule;
impl TuningRule for TomcatConnectionLimitRule {
    fn id(&self) -> &str {
        "tomcat_connections"
    }
    fn name(&self) -> &str {
        "Tomcat connection limit"
    }
    fn evaluate(&self, current: &JvmSnapshot, _history: &[JvmSnapshot]) -> Option<Recommendation> {
        let t = current.tomcat.as_ref()?;
        if t.connections_max == 0 {
            return None;
        }
        let util = t.connections_current as f64 / t.connections_max as f64;
        if util < 0.8 {
            return None;
        }
        Some(Recommendation {
            id: self.id().to_string(),
            category: Category::WebServer,
            severity: if util > 0.95 {
                Severity::High
            } else {
                Severity::Warn
            },
            title: "Tomcat connections near the configured limit".to_string(),
            current_state: format!(
                "{}/{} connections open ({:.0}%)",
                t.connections_current,
                t.connections_max,
                util * 100.0
            ),
            suggestion: "Raise max-connections, or reduce keep-alive if connections sit idle"
                .to_string(),
            rationale: "New connections queue on the OS accept backlog once the limit is hit"
                .to_string(),
            confidence: 0.8,
            jvm_flags: vec![],
            config_changes: vec![format!(
                "server.tomcat.max-connections={}",
                t.connections_max * 2
            )],
        })
    }
}

pub struct HttpCacheCandidateRule;
impl TuningRule for HttpCacheCandidateRule {
    fn id(&self) -> &str {
        "http_cache_candidate"
    }
    fn name(&self) -> &str {
        "HTTP caching candidate"
    }
    fn evaluate(&self, current: &JvmSnapshot, _history: &[JvmSnapshot]) -> Option<Recommendation> {
        if current.http.total_requests < 1000 {
            return None;
        }
        let hot = current
            .http
            .endpoints
            .iter()
            .filter(|e| e.method == "GET" && e.count > 0)
            .max_by_key(|e| e.count)?;
        let share = hot.count as f64 / current.http.total_requests as f64;
        let avg_ms = if hot.count > 0 {
            hot.total_time_ms / hot.count as f64
        } else {
            0.0
        };
        if share < 0.5 || avg_ms < 10.0 {
            return None;
        }
        Some(Recommendation {
            id: self.id().to_string(),
            category: Category::Http,
            severity: Severity::Info,
            title: format!("'{}' dominates traffic — cache candidate", hot.uri),
            current_state: format!(
                "{:.0}% of all requests ({}), avg {:.1}ms per call",
                share * 100.0,
                hot.count,
                avg_ms
            ),
            suggestion:
                "Add @Cacheable on the handler's service call or HTTP Cache-Control headers"
                    .to_string(),
            rationale: "One hot, read-only endpoint is the cheapest big win caching offers"
                .to_string(),
            confidence: 0.6,
            jvm_flags: vec![],
            config_changes: vec!["spring.cache.type=caffeine".to_string()],
        })
    }
}

pub struct LogVolumeRule;
impl TuningRule for LogVolumeRule {
    fn id(&self) -> &str {
        "log_volume"
    }
    fn name(&self) -> &str {
        "Excessive debug logging"
    }
    fn evaluate(&self, current: &JvmSnapshot, history: &[JvmSnapshot]) -> Option<Recommendation> {
        let debugish = current.log_events.debug + current.log_events.trace;
        if debugish < 10_000 {
            return None;
        }
        // Rate over the observed window, if we have history
        let rate = history.first().map(|oldest| {
            let then = oldest.log_events.debug + oldest.log_events.trace;
            let dt = current
                .timestamp
                .signed_duration_since(oldest.timestamp)
                .num_seconds()
                .max(1);
            (debugish - then) as f64 / dt as f64
        });
        Some(Recommendation {
            id: self.id().to_string(),
            category: Category::Logging,
            severity: Severity::Info,
            title: "High DEBUG/TRACE log volume".to_string(),
            current_state: match rate {
                Some(r) if r > 0.0 => format!(
                    "{} debug/trace events total, ~{:.0}/s currently",
                    debugish, r
                ),
                _ => format!("{} debug/trace events since startup", debugish),
            },
            suggestion: "Raise the root log level; keep DEBUG only for specific loggers"
                .to_string(),
            rationale: "Verbose logging burns CPU, disk, and log-pipeline cost in production"
                .to_string(),
            confidence: 0.6,
            jvm_flags: vec![],
            config_changes: vec!["logging.level.root=INFO".to_string()],
        })
    }
}

/// Create a RecommendationEngine with all built-in rules.
pub fn default_engine() -> RecommendationEngine {
    let mut engine = RecommendationEngine::new(0.5);
    // JVM
    engine.register(Box::new(IncreaseHeapRule));
    engine.register(Box::new(SetXmsEqualsXmxRule));
    engine.register(Box::new(SwitchToZgcRule));
    // Application
    engine.register(Box::new(HikariPoolSizeRule));
    engine.register(Box::new(HikariSlowAcquireRule));
    engine.register(Box::new(TomcatThreadRule));
    engine.register(Box::new(TomcatConnectionLimitRule));
    engine.register(Box::new(ExecutorBacklogRule));
    engine.register(Box::new(ExecutorOversizedRule));
    engine.register(Box::new(CacheHitRatioRule));
    engine.register(Box::new(HttpCacheCandidateRule));
    engine.register(Box::new(LogVolumeRule));
    engine
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap_with_executor(active: i64, pool: i64, queued: i64) -> JvmSnapshot {
        JvmSnapshot {
            executors: vec![ExecutorMetrics {
                name: "applicationTaskExecutor".into(),
                pool_size: pool,
                core_size: pool,
                max_size: pool,
                active,
                queued,
                queue_remaining: Some(100),
                completed_total: 5000,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn executor_backlog_fires_when_saturated_with_queue() {
        let snap = snap_with_executor(8, 8, 20);
        let rec = ExecutorBacklogRule.evaluate(&snap, &[]).unwrap();
        assert_eq!(rec.category, Category::Executor);
        assert!(rec.config_changes[0].contains("spring.task.execution.pool.max-size"));
    }

    #[test]
    fn executor_backlog_silent_when_pool_has_headroom() {
        let snap = snap_with_executor(3, 8, 5);
        assert!(ExecutorBacklogRule.evaluate(&snap, &[]).is_none());
    }

    #[test]
    fn cache_hit_ratio_fires_below_50pct() {
        let snap = JvmSnapshot {
            caches: vec![CacheMetrics {
                name: "books".into(),
                hits: 300,
                misses: 900,
                size: 500,
                evictions: 50,
            }],
            ..Default::default()
        };
        let rec = CacheHitRatioRule.evaluate(&snap, &[]).unwrap();
        assert!(rec.title.contains("books"));
        assert!(rec.config_changes[0].contains("caffeine"));
    }

    #[test]
    fn cache_rule_ignores_cold_caches() {
        let snap = JvmSnapshot {
            caches: vec![CacheMetrics {
                name: "cold".into(),
                hits: 1,
                misses: 5,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(CacheHitRatioRule.evaluate(&snap, &[]).is_none());
    }

    #[test]
    fn hikari_slow_acquire_distinguishes_db_bottleneck() {
        let snap = JvmSnapshot {
            hikari: Some(HikariMetrics {
                pool_name: "main".into(),
                active: 3,
                idle: 7,
                pending: 4,
                total: 10,
                max: 10,
                ..Default::default()
            }),
            ..Default::default()
        };
        let rec = HikariSlowAcquireRule.evaluate(&snap, &[]).unwrap();
        assert!(rec.suggestion.contains("database"));
    }

    #[test]
    fn tomcat_connection_limit_fires_near_max() {
        let snap = JvmSnapshot {
            tomcat: Some(TomcatMetrics {
                threads_busy: 10,
                threads_current: 20,
                threads_max: 200,
                connections_current: 8000,
                connections_max: 8192,
            }),
            ..Default::default()
        };
        let rec = TomcatConnectionLimitRule.evaluate(&snap, &[]).unwrap();
        assert!(rec.config_changes[0].contains("server.tomcat.max-connections"));
    }

    #[test]
    fn log_volume_fires_on_heavy_debug() {
        let snap = JvmSnapshot {
            log_events: LogEventCounts {
                debug: 50_000,
                trace: 5_000,
                ..Default::default()
            },
            ..Default::default()
        };
        let rec = LogVolumeRule.evaluate(&snap, &[]).unwrap();
        assert_eq!(rec.config_changes[0], "logging.level.root=INFO");
    }

    #[test]
    fn default_engine_has_all_rules() {
        assert_eq!(default_engine().rule_count(), 12);
    }
}
