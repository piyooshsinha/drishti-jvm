//! Convert Jolokia bulk responses into JvmSnapshot fields.

use crate::response::JolokiaResponse;
use chrono::Utc;
use drishti_core::model::*;

/// Parse the 8-element standard bulk response into a JvmSnapshot.
pub fn bulk_to_snapshot(responses: &[JolokiaResponse]) -> JvmSnapshot {
    let mut snap = JvmSnapshot {
        timestamp: Utc::now(),
        ..Default::default()
    };
    if responses.len() < 8 {
        return snap;
    }

    // [0] Memory
    if responses[0].is_ok() {
        if let Some(heap) = responses[0].value.get("HeapMemoryUsage") {
            snap.heap = parse_mem(heap);
        }
        if let Some(nh) = responses[0].value.get("NonHeapMemoryUsage") {
            snap.non_heap = parse_mem(nh);
        }
    }

    // [1] Threading
    if responses[1].is_ok() {
        let v = &responses[1].value;
        snap.thread_summary = ThreadSummary {
            live: v.get("ThreadCount").and_then(|v| v.as_i64()).unwrap_or(0),
            daemon: v
                .get("DaemonThreadCount")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            peak: v
                .get("PeakThreadCount")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            ..Default::default()
        };
    }

    // [2] GC Collectors (wildcard)
    if responses[2].is_ok() {
        if let Some(obj) = responses[2].value.as_object() {
            for (name, attrs) in obj {
                let short = extract_mbean_name(name);
                snap.gc_collectors.push(GcCollectorStats {
                    name: short,
                    collection_count: attrs
                        .get("CollectionCount")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0),
                    collection_time_ms: attrs
                        .get("CollectionTime")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0),
                });
            }
        }
    }

    // [3] Memory Pools (wildcard)
    if responses[3].is_ok() {
        if let Some(obj) = responses[3].value.as_object() {
            for (name, attrs) in obj {
                let short = extract_mbean_name(name);
                let pool_type = match attrs.get("Type").and_then(|v| v.as_str()).unwrap_or("") {
                    "HEAP" => PoolType::Heap,
                    "NON_HEAP" => PoolType::NonHeap,
                    _ => PoolType::Unknown,
                };
                snap.memory_pools.push(MemoryPool {
                    name: short,
                    pool_type,
                    usage: attrs.get("Usage").map(parse_mem).unwrap_or_default(),
                    collection_usage: attrs
                        .get("CollectionUsage")
                        .filter(|v| !v.is_null())
                        .map(parse_mem),
                });
            }
        }
    }

    // [4] OperatingSystem
    if responses[4].is_ok() {
        let v = &responses[4].value;
        snap.cpu = CpuMetrics {
            process_cpu: v
                .get("ProcessCpuLoad")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            system_cpu: v
                .get("SystemCpuLoad")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            available_processors: v
                .get("AvailableProcessors")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32,
            system_load_average_1m: v
                .get("SystemLoadAverage")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
        };
    }

    // [5] Runtime
    if responses[5].is_ok() {
        let v = &responses[5].value;
        snap.jvm_info = JvmInfo {
            vm_name: v
                .get("VmName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            vm_vendor: v
                .get("VmVendor")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            vm_version: v
                .get("VmVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            spec_version: v
                .get("SpecVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            uptime_ms: v.get("Uptime").and_then(|v| v.as_i64()).unwrap_or(0),
            input_arguments: v
                .get("InputArguments")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            gc_algorithm: detect_gc(&snap.gc_collectors),
            max_heap_bytes: None,
            initial_heap_bytes: None,
        };
        parse_heap_flags(&mut snap.jvm_info);
    }

    // [6] ClassLoading
    if responses[6].is_ok() {
        let v = &responses[6].value;
        snap.classes = ClassMetrics {
            loaded: v
                .get("LoadedClassCount")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            total_loaded: v
                .get("TotalLoadedClassCount")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            unloaded: v
                .get("UnloadedClassCount")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
        };
    }

    // [7] Deadlock check
    if responses[7].is_ok() && !responses[7].value.is_null() {
        if let Some(ids) = responses[7].value.as_array() {
            let thread_ids: Vec<i64> = ids.iter().filter_map(|v| v.as_i64()).collect();
            if !thread_ids.is_empty() {
                snap.deadlocks.push(DeadlockInfo {
                    thread_ids,
                    threads: vec![],
                    detected_at: Utc::now(),
                });
            }
        }
    }

    snap
}

fn parse_mem(v: &serde_json::Value) -> MemoryUsage {
    MemoryUsage {
        init: v.get("init").and_then(|v| v.as_i64()).unwrap_or(0),
        used: v.get("used").and_then(|v| v.as_i64()).unwrap_or(0),
        committed: v.get("committed").and_then(|v| v.as_i64()).unwrap_or(0),
        max: v.get("max").and_then(|v| v.as_i64()).unwrap_or(-1),
    }
}

fn extract_mbean_name(mbean: &str) -> String {
    // ObjectName is "domain:prop=v,prop=v"; strip the domain so a leading
    // "name=" property (e.g. "java.lang:name=G1 Young Generation,...") matches.
    let props = mbean.split_once(':').map(|(_, p)| p).unwrap_or(mbean);
    props
        .split(',')
        .find(|s| s.starts_with("name="))
        .map(|s| s.trim_start_matches("name=").to_string())
        .unwrap_or_else(|| mbean.to_string())
}

fn detect_gc(collectors: &[GcCollectorStats]) -> GcAlgorithm {
    for c in collectors {
        let n = c.name.to_lowercase();
        if n.contains("zgc") {
            return GcAlgorithm::Zgc;
        }
        if n.contains("shenandoah") {
            return GcAlgorithm::Shenandoah;
        }
        if n.contains("g1") {
            return GcAlgorithm::G1;
        }
        if n.contains("parallel") || n.contains("ps") {
            return GcAlgorithm::Parallel;
        }
    }
    GcAlgorithm::Unknown
}

fn parse_heap_flags(info: &mut JvmInfo) {
    for arg in &info.input_arguments {
        if let Some(v) = arg.strip_prefix("-Xmx") {
            info.max_heap_bytes = Some(parse_size(v));
        } else if let Some(v) = arg.strip_prefix("-Xms") {
            info.initial_heap_bytes = Some(parse_size(v));
        }
    }
}

fn parse_size(s: &str) -> i64 {
    let s = s.trim();
    let (n, m) = if s.ends_with(['g', 'G']) {
        (&s[..s.len() - 1], 1073741824i64)
    } else if s.ends_with(['m', 'M']) {
        (&s[..s.len() - 1], 1048576)
    } else if s.ends_with(['k', 'K']) {
        (&s[..s.len() - 1], 1024)
    } else {
        (s, 1)
    };
    n.parse::<i64>().unwrap_or(0) * m
}
