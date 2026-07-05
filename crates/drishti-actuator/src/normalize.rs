//! Metric Normalization Layer
//!
//! Maps vendor-specific metric names to canonical JvmSnapshot fields.
//! Handles the naming differences across:
//! - Spring Boot 2.x (Micrometer 1.x)
//! - Spring Boot 3.x (Micrometer 1.11+)
//! - Raw Jolokia (JMX MBean attribute names)
//!
//! The UI never touches a raw metric name. It queries JvmSnapshot fields only.
//! This module is the single place where metric name strings are defined.

use std::collections::HashMap;

/// A canonical metric identifier — what the snapshot and UI understand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonicalMetric {
    // Memory
    HeapUsed,
    HeapMax,
    HeapCommitted,
    NonHeapUsed,
    NonHeapCommitted,
    MemoryPoolUsed, // needs {area, id} labels
    MemoryPoolMax,
    MemoryPoolCommitted,

    // CPU
    ProcessCpu,
    SystemCpu,
    SystemCpuCount,
    LoadAverage1m,

    // Threads
    ThreadsLive,
    ThreadsDaemon,
    ThreadsPeak,
    ThreadStates, // needs {state} label

    // GC
    GcPauseCount, // needs {action, cause} labels
    GcPauseSum,
    GcPauseMax,
    GcMemoryAllocated,
    GcMemoryPromoted,

    // Classes
    ClassesLoaded,
    ClassesUnloaded,

    // HTTP
    HttpRequestsCount, // needs {uri, method, status} labels
    HttpRequestsSum,
    HttpRequestsMax,

    // HikariCP
    HikariActive,
    HikariIdle,
    HikariPending,
    HikariTotal,
    HikariMax,
    HikariTimeoutTotal,
    HikariAcquireSum,
    HikariUsageSum,
    HikariCreationSum,

    // Tomcat
    TomcatThreadsBusy,
    TomcatThreadsCurrent,
    TomcatThreadsMax,
}

/// A set of name variants for a single canonical metric.
/// The normalizer tries each variant in order until one matches.
#[derive(Debug, Clone)]
pub struct MetricMapping {
    pub canonical: CanonicalMetric,
    /// Prometheus metric name variants (tried in order).
    pub prometheus_names: Vec<&'static str>,
    /// Jolokia MBean path (mbean:attribute).
    pub jolokia_path: Option<&'static str>,
}

/// The normalization registry — holds all known metric name mappings.
pub struct MetricRegistry {
    /// Prometheus name → canonical metric
    prom_index: HashMap<String, CanonicalMetric>,
    /// All mappings for iteration
    mappings: Vec<MetricMapping>,
}

impl MetricRegistry {
    /// Build the default registry covering Boot 2.x, 3.x, and Jolokia.
    pub fn default_registry() -> Self {
        let mappings = vec![
            // ── Threads ──────────────────────────────────────
            MetricMapping {
                canonical: CanonicalMetric::ThreadsLive,
                prometheus_names: vec![
                    "jvm_threads_live_threads", // Boot 3.x / Micrometer 1.11+
                    "jvm_threads_live",         // Boot 2.x / Micrometer 1.0-1.10
                ],
                jolokia_path: Some("java.lang:type=Threading:ThreadCount"),
            },
            MetricMapping {
                canonical: CanonicalMetric::ThreadsDaemon,
                prometheus_names: vec![
                    "jvm_threads_daemon_threads", // Boot 3.x
                    "jvm_threads_daemon",         // Boot 2.x
                ],
                jolokia_path: Some("java.lang:type=Threading:DaemonThreadCount"),
            },
            MetricMapping {
                canonical: CanonicalMetric::ThreadsPeak,
                prometheus_names: vec![
                    "jvm_threads_peak_threads", // Boot 3.x
                    "jvm_threads_peak",         // Boot 2.x
                ],
                jolokia_path: Some("java.lang:type=Threading:PeakThreadCount"),
            },
            MetricMapping {
                canonical: CanonicalMetric::ThreadStates,
                prometheus_names: vec![
                    "jvm_threads_states_threads", // Both Boot 2.x and 3.x
                ],
                jolokia_path: None,
            },
            // ── CPU ──────────────────────────────────────────
            MetricMapping {
                canonical: CanonicalMetric::ProcessCpu,
                prometheus_names: vec![
                    "process_cpu_usage",         // Both Boot 2.x and 3.x
                    "process_cpu_time_ns_total", // Rare variant
                ],
                jolokia_path: Some("java.lang:type=OperatingSystem:ProcessCpuLoad"),
            },
            MetricMapping {
                canonical: CanonicalMetric::SystemCpu,
                prometheus_names: vec!["system_cpu_usage"],
                jolokia_path: Some("java.lang:type=OperatingSystem:SystemCpuLoad"),
            },
            MetricMapping {
                canonical: CanonicalMetric::SystemCpuCount,
                prometheus_names: vec!["system_cpu_count"],
                jolokia_path: Some("java.lang:type=OperatingSystem:AvailableProcessors"),
            },
            MetricMapping {
                canonical: CanonicalMetric::LoadAverage1m,
                prometheus_names: vec!["system_load_average_1m"],
                jolokia_path: Some("java.lang:type=OperatingSystem:SystemLoadAverage"),
            },
            // ── Memory ──────────────────────────────────────
            MetricMapping {
                canonical: CanonicalMetric::MemoryPoolUsed,
                prometheus_names: vec!["jvm_memory_used_bytes"],
                jolokia_path: None, // Comes from wildcard MBean read
            },
            MetricMapping {
                canonical: CanonicalMetric::MemoryPoolMax,
                prometheus_names: vec!["jvm_memory_max_bytes"],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::MemoryPoolCommitted,
                prometheus_names: vec!["jvm_memory_committed_bytes"],
                jolokia_path: None,
            },
            // ── GC ──────────────────────────────────────────
            MetricMapping {
                canonical: CanonicalMetric::GcPauseCount,
                prometheus_names: vec![
                    "jvm_gc_pause_seconds_count", // Boot 2.x + 3.x
                ],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::GcPauseSum,
                prometheus_names: vec!["jvm_gc_pause_seconds_sum"],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::GcMemoryAllocated,
                prometheus_names: vec![
                    "jvm_gc_memory_allocated_bytes_total", // Boot 3.x
                    "jvm_gc_memory_allocated_bytes",       // Boot 2.x (gauge)
                ],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::GcMemoryPromoted,
                prometheus_names: vec![
                    "jvm_gc_memory_promoted_bytes_total", // Boot 3.x
                    "jvm_gc_memory_promoted_bytes",       // Boot 2.x
                ],
                jolokia_path: None,
            },
            // ── Classes ─────────────────────────────────────
            MetricMapping {
                canonical: CanonicalMetric::ClassesLoaded,
                prometheus_names: vec![
                    "jvm_classes_loaded_classes", // Both
                    "jvm_classes_loaded",         // Rare
                ],
                jolokia_path: Some("java.lang:type=ClassLoading:LoadedClassCount"),
            },
            MetricMapping {
                canonical: CanonicalMetric::ClassesUnloaded,
                prometheus_names: vec![
                    "jvm_classes_unloaded_classes_total", // Boot 3.x
                    "jvm_classes_unloaded_classes",       // Boot 2.x
                    "jvm_classes_unloaded",               // Rare
                ],
                jolokia_path: Some("java.lang:type=ClassLoading:UnloadedClassCount"),
            },
            // ── HTTP ────────────────────────────────────────
            MetricMapping {
                canonical: CanonicalMetric::HttpRequestsCount,
                prometheus_names: vec![
                    "http_server_requests_seconds_count",        // Boot 2.x + 3.x
                    "http_server_requests_active_seconds_count", // Boot 3.2+ observation API
                ],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::HttpRequestsSum,
                prometheus_names: vec![
                    "http_server_requests_seconds_sum",
                    "http_server_requests_active_seconds_sum",
                ],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::HttpRequestsMax,
                prometheus_names: vec!["http_server_requests_seconds_max"],
                jolokia_path: None,
            },
            // ── HikariCP ────────────────────────────────────
            MetricMapping {
                canonical: CanonicalMetric::HikariActive,
                prometheus_names: vec!["hikaricp_connections_active"],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::HikariIdle,
                prometheus_names: vec!["hikaricp_connections_idle"],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::HikariPending,
                prometheus_names: vec!["hikaricp_connections_pending"],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::HikariTotal,
                prometheus_names: vec!["hikaricp_connections"],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::HikariMax,
                prometheus_names: vec!["hikaricp_connections_max"],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::HikariTimeoutTotal,
                prometheus_names: vec!["hikaricp_connections_timeout_total"],
                jolokia_path: None,
            },
            // ── Tomcat ──────────────────────────────────────
            MetricMapping {
                canonical: CanonicalMetric::TomcatThreadsBusy,
                prometheus_names: vec![
                    "tomcat_threads_busy_threads", // Boot 3.x
                    "tomcat_threads_busy",         // Boot 2.x
                ],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::TomcatThreadsCurrent,
                prometheus_names: vec!["tomcat_threads_current_threads", "tomcat_threads_current"],
                jolokia_path: None,
            },
            MetricMapping {
                canonical: CanonicalMetric::TomcatThreadsMax,
                prometheus_names: vec![
                    "tomcat_threads_config_max_threads",
                    "tomcat_threads_config_max",
                ],
                jolokia_path: None,
            },
        ];

        let mut prom_index = HashMap::new();
        for m in &mappings {
            for name in &m.prometheus_names {
                prom_index.insert(name.to_string(), m.canonical);
            }
        }

        Self {
            prom_index,
            mappings,
        }
    }

    /// Look up a Prometheus metric name → canonical metric.
    /// Returns None if the metric name isn't recognized.
    pub fn resolve_prometheus(&self, name: &str) -> Option<CanonicalMetric> {
        self.prom_index.get(name).copied()
    }

    /// Get all Prometheus name variants for a canonical metric.
    pub fn prometheus_names_for(&self, canonical: CanonicalMetric) -> Vec<&str> {
        self.mappings
            .iter()
            .filter(|m| m.canonical == canonical)
            .flat_map(|m| m.prometheus_names.iter().copied())
            .collect()
    }

    /// Find a gauge value by canonical metric, trying all name variants.
    pub fn find_gauge_value(
        &self,
        samples: &[crate::prometheus::Sample],
        canonical: CanonicalMetric,
        labels: &[(&str, &str)],
    ) -> Option<f64> {
        for name in self.prometheus_names_for(canonical) {
            if let Some(val) = crate::prometheus::find_gauge(samples, name, labels) {
                return Some(val);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_boot3_thread_names() {
        let reg = MetricRegistry::default_registry();
        assert_eq!(
            reg.resolve_prometheus("jvm_threads_live_threads"),
            Some(CanonicalMetric::ThreadsLive)
        );
        assert_eq!(
            reg.resolve_prometheus("jvm_threads_daemon_threads"),
            Some(CanonicalMetric::ThreadsDaemon)
        );
    }

    #[test]
    fn resolve_boot2_thread_names() {
        let reg = MetricRegistry::default_registry();
        assert_eq!(
            reg.resolve_prometheus("jvm_threads_live"),
            Some(CanonicalMetric::ThreadsLive)
        );
        assert_eq!(
            reg.resolve_prometheus("jvm_threads_daemon"),
            Some(CanonicalMetric::ThreadsDaemon)
        );
    }

    #[test]
    fn resolve_unknown_returns_none() {
        let reg = MetricRegistry::default_registry();
        assert_eq!(reg.resolve_prometheus("custom_app_metric"), None);
    }

    #[test]
    fn get_all_variants() {
        let reg = MetricRegistry::default_registry();
        let names = reg.prometheus_names_for(CanonicalMetric::ThreadsLive);
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"jvm_threads_live_threads"));
        assert!(names.contains(&"jvm_threads_live"));
    }

    #[test]
    fn tomcat_naming_across_versions() {
        let reg = MetricRegistry::default_registry();
        // Boot 3.x name
        assert_eq!(
            reg.resolve_prometheus("tomcat_threads_busy_threads"),
            Some(CanonicalMetric::TomcatThreadsBusy)
        );
        // Boot 2.x name
        assert_eq!(
            reg.resolve_prometheus("tomcat_threads_busy"),
            Some(CanonicalMetric::TomcatThreadsBusy)
        );
    }

    #[test]
    fn http_observation_api_variant() {
        let reg = MetricRegistry::default_registry();
        // Classic
        assert_eq!(
            reg.resolve_prometheus("http_server_requests_seconds_count"),
            Some(CanonicalMetric::HttpRequestsCount)
        );
        // Boot 3.2+ observation API
        assert_eq!(
            reg.resolve_prometheus("http_server_requests_active_seconds_count"),
            Some(CanonicalMetric::HttpRequestsCount)
        );
    }
}
