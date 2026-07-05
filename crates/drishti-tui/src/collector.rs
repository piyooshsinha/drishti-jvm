//! Data collector — manages shared state and async polling tasks.

use arc_swap::ArcSwap;
use drishti_actuator::ActuatorClient;
use drishti_actuator::client::ActuatorAuth;
use drishti_actuator::converter::prometheus_to_snapshot;
use drishti_actuator::logfile::spawn_remote_log_tailer;
use drishti_actuator::threads::parse_thread_dump;
use drishti_jolokia::JolokiaClient;
use drishti_jolokia::client::JolokiaAuth;
use drishti_jolokia::converter::bulk_to_snapshot;
use drishti_jolokia::request::BulkRequestBuilder;
use drishti_core::model::{JvmSnapshot, GcAlgorithm, DerivedMetrics, GcEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, mpsc};
use tokio_util::sync::CancellationToken;

pub struct AppState {
    pub snapshot: Arc<ArcSwap<JvmSnapshot>>,
    pub history: Arc<std::sync::Mutex<Vec<JvmSnapshot>>>,
    pub derived: Arc<ArcSwap<DerivedMetrics>>,
    pub gc_events: Arc<std::sync::Mutex<Vec<GcEvent>>>,
    pub watch_rx: watch::Receiver<u64>,
    pub readonly: bool,
    watch_tx: watch::Sender<u64>,
    tick: std::sync::atomic::AtomicU64,
    prev_snapshot: Arc<std::sync::Mutex<Option<JvmSnapshot>>>,
}

impl AppState {
    pub fn new(readonly: bool) -> Self {
        let (watch_tx, watch_rx) = watch::channel(0u64);
        Self {
            snapshot: Arc::new(ArcSwap::from_pointee(JvmSnapshot::default())),
            history: Arc::new(std::sync::Mutex::new(Vec::with_capacity(1000))),
            derived: Arc::new(ArcSwap::from_pointee(DerivedMetrics::default())),
            gc_events: Arc::new(std::sync::Mutex::new(Vec::with_capacity(500))),
            watch_rx, readonly, watch_tx,
            tick: std::sync::atomic::AtomicU64::new(0),
            prev_snapshot: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn current(&self) -> Arc<JvmSnapshot> { self.snapshot.load_full() }
    pub fn current_derived(&self) -> Arc<DerivedMetrics> { self.derived.load_full() }

    pub fn update_snapshot(&self, snap: JvmSnapshot) {
        if let Ok(mut prev) = self.prev_snapshot.lock() {
            if let Some(ref old) = *prev {
                self.derived.store(Arc::new(compute_derived(old, &snap)));
            }
            *prev = Some(snap.clone());
        }
        if let Ok(mut hist) = self.history.lock() {
            hist.push(snap.clone());
            let excess = hist.len().saturating_sub(500);
            if excess > 0 { hist.drain(..excess); }
        }
        self.snapshot.store(Arc::new(snap));
        let t = self.tick.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let _ = self.watch_tx.send(t + 1);
    }

    pub fn add_gc_event(&self, event: GcEvent) {
        if let Ok(mut events) = self.gc_events.lock() {
            events.push(event);
            let excess = events.len().saturating_sub(500);
            if excess > 0 { events.drain(..excess); }
        }
    }

    pub fn get_gc_events(&self) -> Vec<GcEvent> {
        self.gc_events.lock().map(|e| e.clone()).unwrap_or_default()
    }

    pub fn get_history(&self) -> Vec<JvmSnapshot> {
        self.history.lock().map(|h| h.clone()).unwrap_or_default()
    }
}

fn compute_derived(old: &JvmSnapshot, new: &JvmSnapshot) -> DerivedMetrics {
    let dt_ms = new.timestamp.signed_duration_since(old.timestamp).num_milliseconds().max(1) as f64;
    let dt_secs = dt_ms / 1000.0;

    let old_gc_ms: i64 = old.gc_collectors.iter().map(|c| c.collection_time_ms).sum();
    let new_gc_ms: i64 = new.gc_collectors.iter().map(|c| c.collection_time_ms).sum();
    let gc_delta_ms = (new_gc_ms - old_gc_ms).max(0) as f64;

    DerivedMetrics {
        allocation_rate_bytes_per_sec: if dt_secs > 0.0 {
            (new.heap.used - old.heap.used).max(0) as f64 / dt_secs
        } else { 0.0 },
        promotion_rate_bytes_per_sec: 0.0,
        gc_throughput: if dt_ms > 0.0 { 1.0 - (gc_delta_ms / dt_ms) } else { 1.0 },
        gc_overhead: if dt_ms > 0.0 { gc_delta_ms / dt_ms } else { 0.0 },
        http_requests_per_sec: if dt_secs > 0.0 {
            (new.http.total_requests - old.http.total_requests).max(0) as f64 / dt_secs
        } else { 0.0 },
        http_error_rate: {
            let req_delta = (new.http.total_requests - old.http.total_requests).max(0) as f64;
            let err_delta = (new.http.total_errors - old.http.total_errors).max(0) as f64;
            if req_delta > 0.0 { err_delta / req_delta } else { 0.0 }
        },
    }
}

/// Channels through which background collectors feed UI-owned buffers.
pub struct CollectorChannels {
    /// Raw application log lines from the remote `/actuator/logfile` tailer.
    pub log_rx: mpsc::Receiver<String>,
    /// One-time list of MBean names from the Jolokia `search *:*` request.
    pub mbeans_rx: mpsc::Receiver<Vec<String>>,
}

pub fn spawn_collectors(
    state: Arc<AppState>,
    actuator_url: Option<String>,
    jolokia_url: Option<String>,
    gc_log_path: Option<String>,
    cancel: CancellationToken,
) -> CollectorChannels {
    let (log_tx, log_rx) = mpsc::channel::<String>(500);
    let (mbeans_tx, mbeans_rx) = mpsc::channel::<Vec<String>>(1);

    // Task 1: Actuator prometheus scrape (2s)
    if let Some(url) = actuator_url.clone() {
        let state = state.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let client = ActuatorClient::new(&url, ActuatorAuth::None, Duration::from_secs(10));
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {
                        if let Ok(text) = client.scrape_prometheus_raw().await {
                            state.update_snapshot(prometheus_to_snapshot(&text));
                        }
                    }
                }
            }
        });
    }

    // Task 2: Jolokia bulk read (3s)
    if let Some(url) = jolokia_url.clone() {
        let state = state.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let client = JolokiaClient::new(&url, JolokiaAuth::None, Duration::from_secs(10));
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {
                        if let Ok(responses) = client.bulk_read(&BulkRequestBuilder::standard()).await {
                            let current = state.current();
                            state.update_snapshot(merge_snapshots(&current, &bulk_to_snapshot(&responses)));
                        }
                    }
                }
            }
        });
    }

    // Task 3: Thread dump (10s)
    if let Some(url) = actuator_url.clone() {
        let state = state.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let client = ActuatorClient::new(&url, ActuatorAuth::None, Duration::from_secs(15));
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {
                        if let Ok(json) = client.thread_dump_raw().await {
                            if let Ok(threads) = parse_thread_dump(&json) {
                                let mut snap = (*state.current()).clone();
                                snap.threads = threads;
                                snap.timestamp = chrono::Utc::now();
                                state.update_snapshot(snap);
                            }
                        }
                    }
                }
            }
        });
    }

    // Task 4: GC log tailer
    if let Some(path_str) = gc_log_path {
        let state = state.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let (tx, mut rx) = mpsc::channel::<GcEvent>(100);
            let path = PathBuf::from(&path_str);
            let tc = cancel.clone();
            tokio::spawn(async move {
                let _ = drishti_gclog::tailer::tail_gc_log(&path, tx, tc).await;
            });
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    event = rx.recv() => {
                        if let Some(ev) = event {
                            state.add_gc_event(ev.clone());
                            let mut snap = (*state.current()).clone();
                            snap.recent_gc_events.push(ev);
                            if snap.recent_gc_events.len() > 50 {
                                snap.recent_gc_events.drain(..snap.recent_gc_events.len() - 50);
                            }
                            state.update_snapshot(snap);
                        } else { break; }
                    }
                }
            }
        });
    }

    // Task 5: Remote application log tailer via /actuator/logfile
    if let Some(url) = actuator_url {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let (chunk_tx, mut chunk_rx) = mpsc::channel(100);
            let tc = cancel.clone();
            tokio::spawn(spawn_remote_log_tailer(url, ActuatorAuth::None, chunk_tx, tc));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    chunk = chunk_rx.recv() => match chunk {
                        Some(c) => {
                            for line in c.text.lines() {
                                if line.trim().is_empty() { continue; }
                                if log_tx.send(line.to_string()).await.is_err() { return; }
                            }
                        }
                        None => break,
                    }
                }
            }
        });
    }

    // Task 6: One-time Jolokia MBean search for the MBeans tab tree
    if let Some(url) = jolokia_url {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let client = JolokiaClient::new(&url, JolokiaAuth::None, Duration::from_secs(15));
            let request = BulkRequestBuilder::new().search("*:*").build();
            // Retry every 10s until the search succeeds (target may not be up yet)
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {
                        if let Ok(responses) = client.bulk_read(&request).await {
                            if let Some(names) = responses.first()
                                .and_then(|r| r.parse_value::<Vec<String>>().ok())
                            {
                                let _ = mbeans_tx.send(names).await;
                                break;
                            }
                        }
                    }
                }
            }
        });
    }

    CollectorChannels { log_rx, mbeans_rx }
}

fn merge_snapshots(actuator: &JvmSnapshot, jolokia: &JvmSnapshot) -> JvmSnapshot {
    let mut m = actuator.clone();
    if !jolokia.deadlocks.is_empty() { m.deadlocks = jolokia.deadlocks.clone(); }
    if !jolokia.jvm_info.vm_name.is_empty() {
        m.jvm_info.vm_name = jolokia.jvm_info.vm_name.clone();
        m.jvm_info.vm_vendor = jolokia.jvm_info.vm_vendor.clone();
        m.jvm_info.vm_version = jolokia.jvm_info.vm_version.clone();
        m.jvm_info.spec_version = jolokia.jvm_info.spec_version.clone();
        m.jvm_info.input_arguments = jolokia.jvm_info.input_arguments.clone();
        m.jvm_info.max_heap_bytes = jolokia.jvm_info.max_heap_bytes;
        m.jvm_info.initial_heap_bytes = jolokia.jvm_info.initial_heap_bytes;
    }
    if jolokia.jvm_info.uptime_ms > 0 { m.jvm_info.uptime_ms = jolokia.jvm_info.uptime_ms; }
    if jolokia.jvm_info.gc_algorithm != GcAlgorithm::Unknown {
        m.jvm_info.gc_algorithm = jolokia.jvm_info.gc_algorithm.clone();
    }
    if jolokia.memory_pools.len() > m.memory_pools.len() {
        m.memory_pools = jolokia.memory_pools.clone();
    }
    m.timestamp = chrono::Utc::now();
    m
}
