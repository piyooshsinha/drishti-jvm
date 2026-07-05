use drishti_core::model::{GcAlgorithm, GcPhase};
use drishti_gclog::detect_algorithm;
use drishti_gclog::g1::parse_g1_log;
use drishti_gclog::shenandoah::parse_shenandoah_log;
use drishti_gclog::zgc::parse_zgc_log;

fn load_fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {}: {}", path, e))
}

#[test]
fn parse_g1_log_fixture() {
    let text = load_fixture("g1_sample.log");
    let events = parse_g1_log(&text);

    assert_eq!(events.len(), 5, "Should parse 5 G1 events");

    // First event: young pause
    assert_eq!(events[0].id, 0);
    assert_eq!(events[0].phase, GcPhase::YoungPause);
    assert_eq!(events[0].heap_before_bytes, 24 * 1024 * 1024);
    assert!((events[0].pause_ms - 2.345).abs() < 0.001);

    // Mixed pause
    assert_eq!(events[3].phase, GcPhase::MixedPause);

    // Full GC — critical event
    assert_eq!(events[4].phase, GcPhase::FullGc);
    assert!((events[4].pause_ms - 567.890).abs() < 0.001);
    assert_eq!(events[4].heap_before_bytes, 480 * 1024 * 1024);
    assert_eq!(events[4].heap_after_bytes, 120 * 1024 * 1024);
}

#[test]
fn detect_g1_from_fixture() {
    let text = load_fixture("g1_sample.log");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(detect_algorithm(&lines), GcAlgorithm::G1);
}

#[test]
fn parse_zgc_pause_lines() {
    let log = "\
[2024-01-15T10:30:00.000+0000][5.0s][info][gc] GC(0) Pause Mark Start 0.012ms
[2024-01-15T10:30:00.100+0000][5.1s][info][gc] GC(0) Pause Mark End 0.008ms
[2024-01-15T10:30:00.200+0000][5.2s][info][gc] GC(0) Pause Relocate Start 0.005ms
[2024-01-15T10:30:01.000+0000][6.0s][info][gc] GC(0) Garbage Collection (Allocation Rate) 3304M(20%)->384M(2%)
";
    let events = parse_zgc_log(log);
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].phase, GcPhase::InitMark);
    assert!((events[0].pause_ms - 0.012).abs() < 0.001);
    assert_eq!(events[3].heap_before_bytes, 3304 * 1024 * 1024);
}

#[test]
fn parse_zgc_generational() {
    let log = "[2024-01-15T10:30:00.000+0000][5.0s][info][gc] GC(0) Young Garbage Collection (Allocation Rate) 512M->128M\n";
    let events = parse_zgc_log(log);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].phase, GcPhase::YoungPause);
    assert!(events[0].collector.contains("Young"));
}

#[test]
fn parse_shenandoah_phases() {
    let log = "\
[2024-01-15T10:30:00.000+0000][5.0s][info][gc] GC(3) Pause Init Mark (process weakrefs) 0.234ms
[2024-01-15T10:30:00.500+0000][5.5s][info][gc] GC(3) Concurrent marking 400M->410M(512M) 12.345ms
[2024-01-15T10:30:01.000+0000][6.0s][info][gc] GC(3) Pause Final Mark (process weakrefs) 0.567ms
[2024-01-15T10:30:01.500+0000][6.5s][info][gc] GC(3) Concurrent evacuation 410M->350M(512M) 8.901ms
";
    let events = parse_shenandoah_log(log);
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].phase, GcPhase::InitMark);
    assert_eq!(events[1].phase, GcPhase::ConcurrentMark);
    assert_eq!(events[2].phase, GcPhase::FinalMark);
    assert_eq!(events[3].phase, GcPhase::ConcurrentEvacuate);
}

#[test]
fn detect_shenandoah_degenerated() {
    let log = "[2024-01-15T10:35:00.000+0000][10.0s][info][gc] GC(5) Pause Full (Allocation Failure) 480M->120M(512M) 567.890ms\n";
    let events = parse_shenandoah_log(log);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].phase, GcPhase::DegeneratedGc);
}
