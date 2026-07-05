//! # drishti-jvm Data Model
//!
//! Every struct that flows between data collectors and UI tabs.
//! Built in dependency order — each type depends only on types above it.
//!
//! These are designed to be:
//! - `Deserialize`-able from Jolokia/Actuator JSON (tested against saved fixtures)
//! - `Clone + Send + Sync` for safe sharing across tokio tasks
//! - Lightweight enough to store thousands of snapshots in ring buffers

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════
// 1. Memory
// ═══════════════════════════════════════════════════════════════

/// Raw JMX MemoryUsage shape — matches both Jolokia and Actuator responses.
/// Fields are in bytes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub init: i64,
    pub used: i64,
    pub committed: i64,
    /// -1 means undefined (common for non-heap pools like CodeCache)
    pub max: i64,
}

impl MemoryUsage {
    /// Usage as a percentage of max. Returns None if max is undefined (-1 or 0).
    pub fn usage_pct(&self) -> Option<f64> {
        if self.max <= 0 {
            None
        } else {
            Some(self.used as f64 / self.max as f64 * 100.0)
        }
    }

    pub fn used_mb(&self) -> f64 {
        self.used as f64 / 1_048_576.0
    }

    pub fn max_mb(&self) -> f64 {
        self.max as f64 / 1_048_576.0
    }
}

/// JVM memory pool type — heap or non-heap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Default)]
pub enum PoolType {
    Heap,
    NonHeap,
    #[serde(other)]
    #[default]
    Unknown,
}

/// A single JVM memory pool (e.g., "G1 Eden Space", "Metaspace", "CodeCache").
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryPool {
    pub name: String,
    pub pool_type: PoolType,
    pub usage: MemoryUsage,
    /// Post-GC usage — only meaningful for heap pools, None for non-heap.
    pub collection_usage: Option<MemoryUsage>,
}

// ═══════════════════════════════════════════════════════════════
// 2. Garbage Collection
// ═══════════════════════════════════════════════════════════════

/// Cumulative GC collector stats from JMX GarbageCollectorMXBean.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GcCollectorStats {
    pub name: String,
    pub collection_count: i64,
    pub collection_time_ms: i64,
}

/// A single GC pause event parsed from GC logs or computed from Jolokia deltas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcEvent {
    pub id: u64,
    pub collector: String,
    pub cause: String,
    pub phase: GcPhase,
    pub heap_before_bytes: i64,
    pub heap_after_bytes: i64,
    pub heap_capacity_bytes: i64,
    pub pause_ms: f64,
    pub timestamp: DateTime<Utc>,
}

/// GC phase classification — normalised across G1, ZGC, Shenandoah.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GcPhase {
    YoungPause,
    MixedPause,
    FullGc,
    ConcurrentMark,
    ConcurrentEvacuate,
    ConcurrentRelocate,
    ConcurrentUpdateRefs,
    InitMark,
    FinalMark,
    InitUpdateRefs,
    FinalUpdateRefs,
    DegeneratedGc,
    Other(String),
}

impl Default for GcPhase {
    fn default() -> Self {
        Self::Other("unknown".to_string())
    }
}

/// Detected GC algorithm family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GcAlgorithm {
    Serial,
    Parallel,
    G1,
    Zgc,
    ZgcGenerational,
    Shenandoah,
    #[default]
    Unknown,
}

// ═══════════════════════════════════════════════════════════════
// 3. Threads
// ═══════════════════════════════════════════════════════════════

/// JVM thread state — matches java.lang.Thread.State.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Default)]
pub enum ThreadState {
    New,
    Runnable,
    Blocked,
    Waiting,
    TimedWaiting,
    Terminated,
    #[serde(other)]
    #[default]
    Unknown,
}

/// Information about a single JVM thread.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub id: i64,
    pub name: String,
    pub state: ThreadState,
    pub daemon: bool,
    pub cpu_time_ns: Option<i64>,
    pub blocked_count: i64,
    pub waited_count: i64,
    pub lock_name: Option<String>,
    pub lock_owner_name: Option<String>,
    pub lock_owner_id: Option<i64>,
    pub stack_frames: Vec<String>,
    pub in_native: bool,
    pub suspended: bool,
}

/// Describes a deadlock cycle detected via ThreadMXBean.findDeadlockedThreads().
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadlockInfo {
    pub thread_ids: Vec<i64>,
    pub threads: Vec<ThreadInfo>,
    pub detected_at: DateTime<Utc>,
}

/// Summary of thread states (for the overview gauges).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub live: i64,
    pub daemon: i64,
    pub peak: i64,
    pub state_counts: HashMap<ThreadState, i64>,
}

// ═══════════════════════════════════════════════════════════════
// 4. CPU
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CpuMetrics {
    /// JVM process CPU usage (0.0 – 1.0)
    pub process_cpu: f64,
    /// System-wide CPU usage (0.0 – 1.0)
    pub system_cpu: f64,
    pub available_processors: i32,
    pub system_load_average_1m: f64,
}

impl CpuMetrics {
    pub fn process_cpu_pct(&self) -> f64 {
        self.process_cpu * 100.0
    }

    pub fn system_cpu_pct(&self) -> f64 {
        self.system_cpu * 100.0
    }
}

// ═══════════════════════════════════════════════════════════════
// 5. Class Loading
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassMetrics {
    pub loaded: i64,
    pub total_loaded: i64,
    pub unloaded: i64,
}

// ═══════════════════════════════════════════════════════════════
// 6. HTTP (Spring Boot specific)
// ═══════════════════════════════════════════════════════════════

/// Metrics for a single HTTP endpoint (uri + method combo).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HttpEndpointMetrics {
    pub uri: String,
    pub method: String,
    pub count: i64,
    pub total_time_ms: f64,
    pub max_ms: f64,
    pub error_count: i64,
    /// Percentile latencies — computed from Prometheus histogram/summary.
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
}

/// Aggregate HTTP stats across all endpoints.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HttpSummary {
    pub total_requests: i64,
    pub total_errors: i64,
    /// Requests per second (computed from delta between snapshots).
    pub request_rate: f64,
    pub error_rate: f64,
    pub avg_latency_ms: f64,
    pub endpoints: Vec<HttpEndpointMetrics>,
}

// ═══════════════════════════════════════════════════════════════
// 7. HikariCP
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HikariMetrics {
    pub pool_name: String,
    pub active: i32,
    pub idle: i32,
    pub pending: i32,
    pub total: i32,
    pub max: i32,
    pub acquire_ms: f64,
    pub usage_ms: f64,
    pub creation_ms: f64,
    pub timeout_count: i64,
}

impl HikariMetrics {
    pub fn utilization_pct(&self) -> f64 {
        if self.max == 0 {
            0.0
        } else {
            self.active as f64 / self.max as f64 * 100.0
        }
    }

    pub fn is_saturated(&self) -> bool {
        self.active >= self.max && self.pending > 0
    }
}

// ═══════════════════════════════════════════════════════════════
// 8. Tomcat Threads
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TomcatMetrics {
    pub threads_busy: i32,
    pub threads_current: i32,
    pub threads_max: i32,
}

// ═══════════════════════════════════════════════════════════════
// 9. JVM Runtime Info
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JvmInfo {
    pub vm_name: String,
    pub vm_vendor: String,
    pub vm_version: String,
    pub spec_version: String,
    pub uptime_ms: i64,
    pub input_arguments: Vec<String>,
    pub gc_algorithm: GcAlgorithm,
    /// Configured -Xmx in bytes (parsed from input_arguments).
    pub max_heap_bytes: Option<i64>,
    /// Configured -Xms in bytes.
    pub initial_heap_bytes: Option<i64>,
}

impl JvmInfo {
    pub fn uptime_human(&self) -> String {
        let secs = self.uptime_ms / 1000;
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if hours > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}m", mins)
        }
    }

    pub fn java_major_version(&self) -> Option<u32> {
        if let Some(v) = self.spec_version.strip_prefix("1.") {
            v.parse().ok()
        } else {
            self.spec_version.split('.').next()?.parse().ok()
        }
    }

    pub fn gc_algorithm_str(&self) -> &str {
        match self.gc_algorithm {
            GcAlgorithm::G1 => "G1GC",
            GcAlgorithm::Zgc => "ZGC",
            GcAlgorithm::ZgcGenerational => "GenZGC",
            GcAlgorithm::Shenandoah => "Shenandoah",
            GcAlgorithm::Parallel => "ParallelGC",
            GcAlgorithm::Serial => "SerialGC",
            GcAlgorithm::Unknown => "GC:?",
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// 10. Health
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Default)]
pub enum HealthStatus {
    Up,
    Down,
    OutOfService,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthInfo {
    pub status: HealthStatus,
    pub components: HashMap<String, HealthStatus>,
}

// ═══════════════════════════════════════════════════════════════
// 11. Connection Target
// ═══════════════════════════════════════════════════════════════

/// Describes the connection to a target JVM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConnection {
    pub name: String,
    pub actuator_url: Option<String>,
    pub jolokia_url: Option<String>,
    pub gc_log_path: Option<String>,
    pub app_log_path: Option<String>,
    pub connected_since: Option<DateTime<Utc>>,
}

impl Default for TargetConnection {
    fn default() -> Self {
        Self {
            name: "localhost".to_string(),
            actuator_url: None,
            jolokia_url: None,
            gc_log_path: None,
            app_log_path: None,
            connected_since: None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// 12. The Master Snapshot
// ═══════════════════════════════════════════════════════════════

/// A complete point-in-time snapshot of a JVM's state.
///
/// This is the central type that collectors produce and UI tabs consume.
/// It is stored in an `Arc<ArcSwap<JvmSnapshot>>` shared across all tasks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JvmSnapshot {
    pub timestamp: DateTime<Utc>,
    pub target: TargetConnection,

    // ── Memory ────────────────────────────────────────────
    pub heap: MemoryUsage,
    pub non_heap: MemoryUsage,
    pub memory_pools: Vec<MemoryPool>,

    // ── GC ────────────────────────────────────────────────
    pub gc_collectors: Vec<GcCollectorStats>,
    pub recent_gc_events: Vec<GcEvent>,

    // ── Threads ───────────────────────────────────────────
    pub thread_summary: ThreadSummary,
    pub threads: Vec<ThreadInfo>,
    pub deadlocks: Vec<DeadlockInfo>,

    // ── CPU ───────────────────────────────────────────────
    pub cpu: CpuMetrics,

    // ── Classes ───────────────────────────────────────────
    pub classes: ClassMetrics,

    // ── HTTP ──────────────────────────────────────────────
    pub http: HttpSummary,

    // ── HikariCP ──────────────────────────────────────────
    pub hikari: Option<HikariMetrics>,

    // ── Tomcat ────────────────────────────────────────────
    pub tomcat: Option<TomcatMetrics>,

    // ── JVM Info ──────────────────────────────────────────
    pub jvm_info: JvmInfo,

    // ── Health ────────────────────────────────────────────
    pub health: HealthInfo,
}

// ═══════════════════════════════════════════════════════════════
// Derived metrics (computed from snapshot deltas)
// ═══════════════════════════════════════════════════════════════

/// Metrics computed by diffing consecutive JvmSnapshots.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DerivedMetrics {
    /// Bytes allocated per second (computed from young GC heap transitions).
    pub allocation_rate_bytes_per_sec: f64,
    /// Bytes promoted to old gen per second.
    pub promotion_rate_bytes_per_sec: f64,
    /// GC throughput: (wall_time - gc_time) / wall_time over the window.
    pub gc_throughput: f64,
    /// GC overhead: gc_time / wall_time.
    pub gc_overhead: f64,
    /// HTTP requests per second delta.
    pub http_requests_per_sec: f64,
    /// HTTP error rate (errors / total).
    pub http_error_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_usage_pct_normal() {
        let m = MemoryUsage {
            init: 0,
            used: 256 * 1024 * 1024,
            committed: 512 * 1024 * 1024,
            max: 512 * 1024 * 1024,
        };
        assert!((m.usage_pct().unwrap() - 50.0).abs() < 0.01);
        assert!((m.used_mb() - 256.0).abs() < 0.01);
    }

    #[test]
    fn memory_usage_pct_undefined_max() {
        let m = MemoryUsage {
            init: 0,
            used: 100,
            committed: 200,
            max: -1,
        };
        assert!(m.usage_pct().is_none());
    }

    #[test]
    fn hikari_saturation() {
        let h = HikariMetrics {
            active: 10,
            max: 10,
            pending: 3,
            ..Default::default()
        };
        assert!(h.is_saturated());
        assert!((h.utilization_pct() - 100.0).abs() < 0.01);
    }

    #[test]
    fn jvm_info_uptime_human() {
        let info = JvmInfo {
            uptime_ms: 7_380_000, // 2h 3m
            ..Default::default()
        };
        assert_eq!(info.uptime_human(), "2h 3m");
    }

    #[test]
    fn jvm_info_java_version() {
        let mut info = JvmInfo {
            spec_version: "21".to_string(),
            ..Default::default()
        };
        assert_eq!(info.java_major_version(), Some(21));

        info.spec_version = "1.8".to_string();
        assert_eq!(info.java_major_version(), Some(8));
    }

    #[test]
    fn default_snapshot_is_zero() {
        let snap = JvmSnapshot::default();
        assert_eq!(snap.heap.used, 0);
        assert_eq!(snap.thread_summary.live, 0);
        assert!(snap.hikari.is_none());
    }
}
