//! Convert Actuator prometheus scrape into JvmSnapshot fields.
//!
//! Uses MetricRegistry to resolve metric names across Boot 2.x/3.x versions.

use crate::normalize::{CanonicalMetric, MetricRegistry};
use crate::prometheus::parse_prometheus_text;
use drishti_core::model::*;
use std::collections::HashMap;

/// Parse prometheus text into JvmSnapshot using the normalization registry.
pub fn prometheus_to_snapshot(text: &str) -> JvmSnapshot {
    let samples = parse_prometheus_text(text);
    let reg = MetricRegistry::default_registry();
    let mut snap = JvmSnapshot::default();

    // ── Heap & Non-heap (summed from per-pool metrics) ──
    let heap_used: f64 = samples
        .iter()
        .filter(|s| {
            reg.resolve_prometheus(&s.name) == Some(CanonicalMetric::MemoryPoolUsed)
                && s.labels.get("area").map(|a| a.as_str()) == Some("heap")
        })
        .map(|s| s.value)
        .sum();
    let heap_max: f64 = samples
        .iter()
        .filter(|s| {
            reg.resolve_prometheus(&s.name) == Some(CanonicalMetric::MemoryPoolMax)
                && s.labels.get("area").map(|a| a.as_str()) == Some("heap")
        })
        .filter(|s| s.value > 0.0) // Ignore -1 (undefined)
        .map(|s| s.value)
        .sum();
    let heap_committed: f64 = samples
        .iter()
        .filter(|s| {
            reg.resolve_prometheus(&s.name) == Some(CanonicalMetric::MemoryPoolCommitted)
                && s.labels.get("area").map(|a| a.as_str()) == Some("heap")
        })
        .map(|s| s.value)
        .sum();
    let nonheap_used: f64 = samples
        .iter()
        .filter(|s| {
            reg.resolve_prometheus(&s.name) == Some(CanonicalMetric::MemoryPoolUsed)
                && s.labels.get("area").map(|a| a.as_str()) == Some("nonheap")
        })
        .map(|s| s.value)
        .sum();

    snap.heap = MemoryUsage {
        init: 0,
        used: heap_used as i64,
        committed: heap_committed as i64,
        max: if heap_max > 0.0 { heap_max as i64 } else { -1 },
    };
    snap.non_heap = MemoryUsage {
        init: 0,
        used: nonheap_used as i64,
        committed: 0,
        max: -1,
    };

    // ── Memory pools ──
    for s in samples
        .iter()
        .filter(|s| reg.resolve_prometheus(&s.name) == Some(CanonicalMetric::MemoryPoolUsed))
    {
        if let (Some(area), Some(id)) = (s.labels.get("area"), s.labels.get("id")) {
            let pool_type = match area.as_str() {
                "heap" => PoolType::Heap,
                "nonheap" => PoolType::NonHeap,
                _ => PoolType::Unknown,
            };
            let max = reg
                .find_gauge_value(&samples, CanonicalMetric::MemoryPoolMax, &[("id", id)])
                .unwrap_or(-1.0);
            let committed = reg
                .find_gauge_value(
                    &samples,
                    CanonicalMetric::MemoryPoolCommitted,
                    &[("id", id)],
                )
                .unwrap_or(0.0);
            snap.memory_pools.push(MemoryPool {
                name: id.clone(),
                pool_type,
                usage: MemoryUsage {
                    init: 0,
                    used: s.value as i64,
                    committed: committed as i64,
                    max: max as i64,
                },
                collection_usage: None,
            });
        }
    }

    // ── CPU (using registry for name resolution) ──
    snap.cpu = CpuMetrics {
        process_cpu: reg
            .find_gauge_value(&samples, CanonicalMetric::ProcessCpu, &[])
            .unwrap_or(0.0),
        system_cpu: reg
            .find_gauge_value(&samples, CanonicalMetric::SystemCpu, &[])
            .unwrap_or(0.0),
        available_processors: reg
            .find_gauge_value(&samples, CanonicalMetric::SystemCpuCount, &[])
            .unwrap_or(0.0) as i32,
        system_load_average_1m: reg
            .find_gauge_value(&samples, CanonicalMetric::LoadAverage1m, &[])
            .unwrap_or(0.0),
    };

    // ── Threads (tries Boot 3.x names first, falls back to 2.x) ──
    snap.thread_summary = ThreadSummary {
        live: reg
            .find_gauge_value(&samples, CanonicalMetric::ThreadsLive, &[])
            .unwrap_or(0.0) as i64,
        daemon: reg
            .find_gauge_value(&samples, CanonicalMetric::ThreadsDaemon, &[])
            .unwrap_or(0.0) as i64,
        peak: reg
            .find_gauge_value(&samples, CanonicalMetric::ThreadsPeak, &[])
            .unwrap_or(0.0) as i64,
        state_counts: {
            let mut states = HashMap::new();
            for s in samples
                .iter()
                .filter(|s| reg.resolve_prometheus(&s.name) == Some(CanonicalMetric::ThreadStates))
            {
                if let Some(state_str) = s.labels.get("state") {
                    let ts = match state_str.as_str() {
                        "runnable" => ThreadState::Runnable,
                        "blocked" => ThreadState::Blocked,
                        "waiting" => ThreadState::Waiting,
                        "timed-waiting" => ThreadState::TimedWaiting,
                        "new" => ThreadState::New,
                        "terminated" => ThreadState::Terminated,
                        _ => ThreadState::Unknown,
                    };
                    states.insert(ts, s.value as i64);
                }
            }
            states
        },
    };

    // ── Classes ──
    snap.classes = ClassMetrics {
        loaded: reg
            .find_gauge_value(&samples, CanonicalMetric::ClassesLoaded, &[])
            .unwrap_or(0.0) as i64,
        total_loaded: reg
            .find_gauge_value(&samples, CanonicalMetric::ClassesLoaded, &[])
            .unwrap_or(0.0) as i64,
        unloaded: reg
            .find_gauge_value(&samples, CanonicalMetric::ClassesUnloaded, &[])
            .unwrap_or(0.0) as i64,
    };

    // ── HikariCP ──
    let hikari_active = reg.find_gauge_value(&samples, CanonicalMetric::HikariActive, &[]);
    if let Some(active) = hikari_active {
        let pool_name = samples
            .iter()
            .find(|s| reg.resolve_prometheus(&s.name) == Some(CanonicalMetric::HikariActive))
            .and_then(|s| s.labels.get("pool"))
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        snap.hikari = Some(HikariMetrics {
            pool_name,
            active: active as i32,
            idle: reg
                .find_gauge_value(&samples, CanonicalMetric::HikariIdle, &[])
                .unwrap_or(0.0) as i32,
            pending: reg
                .find_gauge_value(&samples, CanonicalMetric::HikariPending, &[])
                .unwrap_or(0.0) as i32,
            total: reg
                .find_gauge_value(&samples, CanonicalMetric::HikariTotal, &[])
                .unwrap_or(0.0) as i32,
            max: reg
                .find_gauge_value(&samples, CanonicalMetric::HikariMax, &[])
                .unwrap_or(0.0) as i32,
            acquire_ms: reg
                .find_gauge_value(&samples, CanonicalMetric::HikariAcquireSum, &[])
                .unwrap_or(0.0)
                * 1000.0,
            usage_ms: reg
                .find_gauge_value(&samples, CanonicalMetric::HikariUsageSum, &[])
                .unwrap_or(0.0)
                * 1000.0,
            creation_ms: reg
                .find_gauge_value(&samples, CanonicalMetric::HikariCreationSum, &[])
                .unwrap_or(0.0)
                * 1000.0,
            timeout_count: reg
                .find_gauge_value(&samples, CanonicalMetric::HikariTimeoutTotal, &[])
                .unwrap_or(0.0) as i64,
        });
    }

    // ── Tomcat ──
    let tomcat_busy = reg.find_gauge_value(&samples, CanonicalMetric::TomcatThreadsBusy, &[]);
    if let Some(busy) = tomcat_busy {
        snap.tomcat = Some(TomcatMetrics {
            threads_busy: busy as i32,
            threads_current: reg
                .find_gauge_value(&samples, CanonicalMetric::TomcatThreadsCurrent, &[])
                .unwrap_or(0.0) as i32,
            threads_max: reg
                .find_gauge_value(&samples, CanonicalMetric::TomcatThreadsMax, &[])
                .unwrap_or(0.0) as i32,
        });
    }

    // ── HTTP endpoints ──
    let mut http_endpoints: HashMap<String, HttpEndpointMetrics> = HashMap::new();
    for s in samples
        .iter()
        .filter(|s| reg.resolve_prometheus(&s.name) == Some(CanonicalMetric::HttpRequestsCount))
    {
        let uri = s.labels.get("uri").cloned().unwrap_or_default();
        let method = s.labels.get("method").cloned().unwrap_or_default();
        let status = s.labels.get("status").cloned().unwrap_or_default();
        let count = s.value as i64;

        let total_time_labels: Vec<(&str, &str)> =
            vec![("uri", &uri), ("method", &method), ("status", &status)]
                .into_iter()
                .map(|(k, v)| (k, v.as_str()))
                .collect();
        let total_time = reg
            .find_gauge_value(
                &samples,
                CanonicalMetric::HttpRequestsSum,
                &total_time_labels,
            )
            .unwrap_or(0.0);
        let max_time = reg
            .find_gauge_value(
                &samples,
                CanonicalMetric::HttpRequestsMax,
                &total_time_labels,
            )
            .unwrap_or(0.0);
        let is_error = status.starts_with('4') || status.starts_with('5');

        let key = format!("{}:{}", method, uri);
        let entry = http_endpoints
            .entry(key)
            .or_insert_with(|| HttpEndpointMetrics {
                uri: uri.clone(),
                method: method.clone(),
                ..Default::default()
            });
        entry.count += count;
        entry.total_time_ms += total_time * 1000.0;
        if max_time * 1000.0 > entry.max_ms {
            entry.max_ms = max_time * 1000.0;
        }
        if is_error {
            entry.error_count += count;
        }
    }

    let endpoints: Vec<HttpEndpointMetrics> = http_endpoints.into_values().collect();
    let total_requests: i64 = endpoints.iter().map(|e| e.count).sum();
    let total_errors: i64 = endpoints.iter().map(|e| e.error_count).sum();
    let total_time: f64 = endpoints.iter().map(|e| e.total_time_ms).sum();

    snap.http = HttpSummary {
        total_requests,
        total_errors,
        request_rate: 0.0,
        error_rate: if total_requests > 0 {
            total_errors as f64 / total_requests as f64
        } else {
            0.0
        },
        avg_latency_ms: if total_requests > 0 {
            total_time / total_requests as f64
        } else {
            0.0
        },
        endpoints,
    };

    snap.timestamp = chrono::Utc::now();
    snap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot2_thread_names_work() {
        let text = "jvm_threads_live 48.0\njvm_threads_daemon 32.0\njvm_threads_peak 52.0\n";
        let snap = prometheus_to_snapshot(text);
        assert_eq!(snap.thread_summary.live, 48);
        assert_eq!(snap.thread_summary.daemon, 32);
    }

    #[test]
    fn boot3_thread_names_work() {
        let text = "jvm_threads_live_threads 48.0\njvm_threads_daemon_threads 32.0\njvm_threads_peak_threads 52.0\n";
        let snap = prometheus_to_snapshot(text);
        assert_eq!(snap.thread_summary.live, 48);
    }

    #[test]
    fn tomcat_boot2_names_work() {
        let text = "tomcat_threads_busy 5.0\ntomcat_threads_current 10.0\ntomcat_threads_config_max 200.0\n";
        let snap = prometheus_to_snapshot(text);
        assert!(snap.tomcat.is_some());
        assert_eq!(snap.tomcat.unwrap().threads_busy, 5);
    }
}
