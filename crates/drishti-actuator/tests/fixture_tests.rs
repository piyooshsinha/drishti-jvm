use drishti_actuator::converter::prometheus_to_snapshot;
use drishti_actuator::health::HealthResponse;
use drishti_actuator::threads::parse_thread_dump;
use drishti_core::model::{HealthStatus, ThreadState};

fn load_fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {}: {}", path, e))
}

#[test]
fn parse_prometheus_fixture() {
    let text = load_fixture("prometheus.txt");
    let snap = prometheus_to_snapshot(&text);

    // Heap
    assert!(
        snap.heap.used > 200_000_000,
        "Heap used should be > 200MB, got {}",
        snap.heap.used
    );

    // CPU
    assert!((snap.cpu.process_cpu - 0.42).abs() < 0.01);
    assert!((snap.cpu.system_cpu - 0.61).abs() < 0.01);

    // Threads
    assert_eq!(snap.thread_summary.live, 48);
    assert_eq!(snap.thread_summary.daemon, 32);
    assert_eq!(
        *snap
            .thread_summary
            .state_counts
            .get(&ThreadState::Runnable)
            .unwrap_or(&0),
        12
    );
    assert_eq!(
        *snap
            .thread_summary
            .state_counts
            .get(&ThreadState::Blocked)
            .unwrap_or(&0),
        2
    );

    // Memory pools
    assert!(!snap.memory_pools.is_empty(), "Should have memory pools");
    let eden = snap.memory_pools.iter().find(|p| p.name.contains("Eden"));
    assert!(eden.is_some(), "Should have Eden pool");

    // HTTP endpoints
    assert!(
        !snap.http.endpoints.is_empty(),
        "Should have HTTP endpoints"
    );
    assert!(
        snap.http.total_requests > 7000,
        "Should have > 7000 requests"
    );
    assert!(
        snap.http.total_errors > 0,
        "Should have some errors from 404s"
    );
    let owners = snap
        .http
        .endpoints
        .iter()
        .find(|e| e.uri.contains("owners"));
    assert!(owners.is_some(), "Should have /owners endpoint");

    // HikariCP
    assert!(snap.hikari.is_some(), "Should have HikariCP metrics");
    let h = snap.hikari.unwrap();
    assert_eq!(h.active, 3);
    assert_eq!(h.idle, 7);
    assert_eq!(h.max, 10);
    assert_eq!(h.pending, 0);
    assert!(!h.is_saturated());

    // Tomcat
    assert!(snap.tomcat.is_some(), "Should have Tomcat metrics");
    let t = snap.tomcat.unwrap();
    assert_eq!(t.threads_busy, 5);
    assert_eq!(t.threads_max, 200);

    // Classes
    assert_eq!(snap.classes.loaded, 12543);
}

#[test]
fn parse_health_boot3() {
    let json = r#"{"status":"UP","components":{"db":{"status":"UP","details":{"database":"H2"}},"diskSpace":{"status":"UP"}}}"#;
    let resp: HealthResponse = serde_json::from_str(json).unwrap();
    let info = resp.to_health_info();
    assert_eq!(info.status, HealthStatus::Up);
    assert_eq!(info.components.len(), 2);
    assert_eq!(info.components.get("db"), Some(&HealthStatus::Up));
}

#[test]
fn parse_thread_dump_fixture() {
    let json = r#"{"threads":[
        {"threadId":1,"threadName":"main","threadState":"RUNNABLE","daemon":false,"blockedCount":3,"waitedCount":7,
         "stackTrace":[{"className":"com.example.App","methodName":"run","fileName":"App.java","lineNumber":42,"nativeMethod":false}]},
        {"threadId":20,"threadName":"http-nio-8080-exec-1","threadState":"WAITING","daemon":true,"blockedCount":0,"waitedCount":50,
         "lockName":"java.util.concurrent.locks.AbstractQueuedSynchronizer$ConditionObject@1a2b3c4d",
         "stackTrace":[{"className":"sun.misc.Unsafe","methodName":"park","nativeMethod":true}]}
    ]}"#;

    let threads = parse_thread_dump(json).unwrap();
    assert_eq!(threads.len(), 2);
    assert_eq!(threads[0].name, "main");
    assert_eq!(threads[0].state, ThreadState::Runnable);
    assert!(!threads[0].daemon);
    assert_eq!(threads[0].blocked_count, 3);
    assert_eq!(threads[1].state, ThreadState::Waiting);
    assert!(threads[1].daemon);
    assert!(threads[1].lock_name.is_some());
}
