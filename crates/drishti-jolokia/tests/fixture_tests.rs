//! Tests that parse saved Jolokia fixtures into JvmSnapshot.

use drishti_jolokia::converter::bulk_to_snapshot;
use drishti_jolokia::response::JolokiaResponse;
use drishti_core::model::GcAlgorithm;

fn load_fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {}: {}", path, e))
}

#[test]
fn parse_bulk_response_fixture() {
    let json = load_fixture("bulk_response.json");
    let responses: Vec<JolokiaResponse> = serde_json::from_str(&json).unwrap();
    assert_eq!(responses.len(), 8, "Expected 8 bulk responses");

    // All should be OK
    for (i, r) in responses.iter().enumerate() {
        assert!(r.is_ok(), "Response {} should be OK, got status {}", i, r.status);
    }

    let snap = bulk_to_snapshot(&responses);

    // Memory
    assert_eq!(snap.heap.used, 268435456, "Heap used should be 256MB");
    assert_eq!(snap.heap.max, 536870912, "Heap max should be 512MB");
    assert!(snap.heap.usage_pct().unwrap() > 49.0 && snap.heap.usage_pct().unwrap() < 51.0,
        "Heap should be ~50%, got {:.1}%", snap.heap.usage_pct().unwrap());
    assert!(snap.non_heap.used > 0, "Non-heap should have data");

    // Threads
    assert_eq!(snap.thread_summary.live, 48);
    assert_eq!(snap.thread_summary.daemon, 32);
    assert_eq!(snap.thread_summary.peak, 52);

    // GC collectors
    assert_eq!(snap.gc_collectors.len(), 2);
    let young = snap.gc_collectors.iter().find(|c| c.name.contains("Young")).unwrap();
    assert_eq!(young.collection_count, 142);
    let old = snap.gc_collectors.iter().find(|c| c.name.contains("Old")).unwrap();
    assert_eq!(old.collection_count, 3);

    // Memory pools
    assert!(snap.memory_pools.len() >= 3, "Should have at least 3 memory pools");
    let eden = snap.memory_pools.iter().find(|p| p.name.contains("Eden")).unwrap();
    assert_eq!(eden.pool_type, drishti_core::model::PoolType::Heap);
    let metaspace = snap.memory_pools.iter().find(|p| p.name.contains("Metaspace")).unwrap();
    assert_eq!(metaspace.pool_type, drishti_core::model::PoolType::NonHeap);

    // CPU
    assert!((snap.cpu.process_cpu - 0.42).abs() < 0.01);
    assert!((snap.cpu.system_cpu - 0.61).abs() < 0.01);
    assert_eq!(snap.cpu.available_processors, 8);

    // Runtime
    assert_eq!(snap.jvm_info.vm_name, "OpenJDK 64-Bit Server VM");
    assert_eq!(snap.jvm_info.spec_version, "21");
    assert_eq!(snap.jvm_info.uptime_ms, 7380000);
    assert_eq!(snap.jvm_info.uptime_human(), "2h 3m");
    assert_eq!(snap.jvm_info.java_major_version(), Some(21));
    assert_eq!(snap.jvm_info.gc_algorithm, GcAlgorithm::G1);
    assert_eq!(snap.jvm_info.max_heap_bytes, Some(512 * 1024 * 1024));
    assert_eq!(snap.jvm_info.initial_heap_bytes, Some(512 * 1024 * 1024));

    // Classes
    assert_eq!(snap.classes.loaded, 12543);
    assert_eq!(snap.classes.unloaded, 57);

    // No deadlocks
    assert!(snap.deadlocks.is_empty());
}

#[test]
fn parse_bulk_with_deadlock() {
    let json = r#"[
        {"status":200,"value":{"HeapMemoryUsage":{"init":0,"used":100,"committed":200,"max":200},"NonHeapMemoryUsage":{"init":0,"used":50,"committed":100,"max":-1}},"request":{},"timestamp":0},
        {"status":200,"value":{"ThreadCount":10,"DaemonThreadCount":5,"PeakThreadCount":12},"request":{},"timestamp":0},
        {"status":200,"value":{},"request":{},"timestamp":0},
        {"status":200,"value":{},"request":{},"timestamp":0},
        {"status":200,"value":{"ProcessCpuLoad":0.1,"SystemCpuLoad":0.2,"AvailableProcessors":4,"SystemLoadAverage":1.0},"request":{},"timestamp":0},
        {"status":200,"value":{"Uptime":1000,"VmName":"Test","VmVendor":"Test","VmVersion":"21","SpecVersion":"21","InputArguments":[]},"request":{},"timestamp":0},
        {"status":200,"value":{"LoadedClassCount":100,"TotalLoadedClassCount":110,"UnloadedClassCount":10},"request":{},"timestamp":0},
        {"status":200,"value":[42,43,44],"request":{},"timestamp":0}
    ]"#;

    let responses: Vec<JolokiaResponse> = serde_json::from_str(json).unwrap();
    let snap = bulk_to_snapshot(&responses);

    assert_eq!(snap.deadlocks.len(), 1);
    assert_eq!(snap.deadlocks[0].thread_ids, vec![42, 43, 44]);
}
