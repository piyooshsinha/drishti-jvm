# दृष्टि drishti-jvm — Project Plan

**Version:** 0.3  
**Date:** July 2026  
**Author:** Piyoosh  
**Status:** Core feature-complete, needs compile verification + integration testing

---

## 1. Project Summary

| Metric | Value |
|--------|-------|
| Total Rust lines | 7,528 |
| Total files | 52 Rust + 6 TOML + 5 scripts + 3 fixtures |
| Crates | 5 (core, jolokia, actuator, gclog, tui) |
| TUI tabs | 10 (all from original architecture spec) |
| Anomaly detectors | 5 |
| Tuning rules | 5 |
| GC parsers | 3 (G1, ZGC, Shenandoah) |
| Test files | 3 integration test files + inline unit tests |

---

## 2. What's DONE ✅

### 2.1 drishti-core (1,991 lines / 9 files)

| File | Status | Description |
|------|--------|-------------|
| `model.rs` | ✅ Complete | 12 struct families: JvmSnapshot, MemoryUsage, MemoryPool, GcCollectorStats, GcEvent, ThreadInfo, DeadlockInfo, HttpEndpointMetrics, HikariMetrics, TomcatMetrics, JvmInfo, DerivedMetrics. All with serde derive, utility methods (usage_pct, uptime_human, java_major_version, gc_algorithm_str), unit tests. |
| `timeseries.rs` | ✅ Complete | Generic `TimeSeries<T>` ring buffer with timestamped entries, linear regression (slope, intercept, R², slope_per_hour, time_to_reach extrapolation), chart data extraction for Ratatui. 5 tests. |
| `anomaly.rs` | ✅ Complete | `AnomalyDetector` trait, `AnomalyEngine` registry, `Alert` type with severity/confidence/evidence_tab, `Severity` enum (Info/Warn/High/Critical). |
| `detectors.rs` | ✅ Complete | 5 detectors: MemoryLeakDetector (linear regression on old gen), GcPressureDetector (Full GC + high heap), DeadlockDetector (Jolokia findDeadlockedThreads), PoolExhaustionDetector (HikariCP saturation/timeouts), HighHeapDetector (>80%/>90% thresholds). |
| `recommend.rs` | ✅ Complete | `TuningRule` trait, `RecommendationEngine` registry with min_confidence filter, `Recommendation` type with copy-pasteable JVM flags. |
| `rules.rs` | ✅ Complete | 5 rules: IncreaseHeapRule (post-GC >70%), SetXmsEqualsXmxRule, SwitchToZgcRule (heap >16GB), HikariPoolSizeRule (cores×2+1), TomcatThreadRule (Little's Law). |
| `persistence.rs` | ✅ Complete | SQLite persistence behind `--features persistence`. Tables: snapshots (15 columns), gc_events, alerts. Insert/query/prune methods with retention. In-memory mode for testing. 2 tests. |
| `targets.rs` | ✅ Complete | Multi-JVM `TargetManager` with per-target snapshots, connection status tracking, staleness detection (30s), metric comparison across instances, target cycling. 3 tests. |

### 2.2 drishti-jolokia (649 lines / 6 files)

| File | Status | Description |
|------|--------|-------------|
| `request.rs` | ✅ Complete | `BulkRequestBuilder` fluent API with `standard()` method producing the 8-element JVM state request. Read/exec/search operations. 2 tests. |
| `response.rs` | ✅ Complete | `JolokiaResponse` envelope with typed `parse_value<T>()`, status checking, error types. 2 tests. |
| `client.rs` | ✅ Complete | `JolokiaClient` with reqwest, Basic/Bearer auth, health_check, bulk_read, connection error handling. |
| `converter.rs` | ✅ Complete | `bulk_to_snapshot()` — parses all 8 bulk response elements into JvmSnapshot (heap, non-heap, pools, GC, CPU, threads, classes, deadlocks, JVM args, GC algorithm detection, -Xmx/-Xms parsing). |
| `tests/fixture_tests.rs` | ✅ Complete | 2 integration tests against saved JSON fixture (8-element bulk response, deadlock response). Tests all snapshot fields. |

### 2.3 drishti-actuator (1,432 lines / 9 files)

| File | Status | Description |
|------|--------|-------------|
| `client.rs` | ✅ Complete | `ActuatorClient` with prometheus scrape, health, threaddump, log-level mutation (POST), auth support. |
| `prometheus.rs` | ✅ Complete | Custom Prometheus exposition format parser (no external dep). `parse_prometheus_text()`, `find_gauge()`. Handles labels, comments, TYPE lines. 4 tests. |
| `converter.rs` | ✅ Complete | `prometheus_to_snapshot()` — fully rewritten to use MetricRegistry for cross-version name resolution. 3 tests (Boot 2 names, Boot 3 names, Tomcat variants). |
| `health.rs` | ✅ Complete | Health response parser supporting both Boot 2.x (`details`) and Boot 3.x (`components`) JSON shapes. 2 tests. |
| `threads.rs` | ✅ Complete | Thread dump parser — `/actuator/threaddump` JSON → `Vec<ThreadInfo>` with stack frames, lock info, state mapping. 1 test. |
| `normalize.rs` | ✅ Complete | `MetricRegistry` with 30+ canonical metrics, each with multiple Prometheus name variants for Boot 2.x/3.x/observation API compatibility. `find_gauge_value()` tries all variants in order. 5 tests. |
| `logfile.rs` | ✅ Complete | Remote log tailing via `/actuator/logfile` with HTTP Range headers. Handles 206/200/416 responses, log rotation detection, spawn helper for background task. |
| `tests/fixture_tests.rs` | ✅ Complete | 3 integration tests against Prometheus fixture (40+ metric lines), health JSON, thread dump JSON. |

### 2.4 drishti-gclog (739 lines / 7 files)

| File | Status | Description |
|------|--------|-------------|
| `lib.rs` | ✅ Complete | `GcAlgorithm` auto-detection from log line samples, `LogLevel` enum, `GcLogError` type. 3 tests. |
| `parser.rs` | ✅ Complete | Unified log prefix parser (ISO-8601 timestamp, uptime, level, tags). Regex-based. 3 tests. |
| `g1.rs` | ✅ Complete | G1GC event parser — Young/Mixed/Full pause extraction with heap transitions. 2 tests. |
| `zgc.rs` | ✅ Complete | ZGC parser — classic + generational (Java 21+) collection summaries, Mark/Relocate pause lines. 3 tests. |
| `shenandoah.rs` | ✅ Complete | Shenandoah parser — all 7 phases (Init Mark → Final Update Refs) + degenerated GC detection. 3 tests. |
| `tailer.rs` | ✅ Complete | Async file tailer with tokio::fs polling, GC algorithm auto-detection, log rotation handling, cancellation token support. |
| `tests/fixture_tests.rs` | ✅ Complete | 5 tests against G1 fixture (5 events), ZGC pauses, ZGC generational, Shenandoah phases, algorithm detection. |

### 2.5 drishti-tui (2,717 lines / 21 files)

| File | Status | Description |
|------|--------|-------------|
| `main.rs` | ✅ Complete | CLI (clap) with --actuator/--jolokia/--gc-log/--readonly/--no-actuator/--no-jolokia flags. Terminal setup with panic-safe restore. Config loading via figment. |
| `app.rs` | ✅ Complete | App struct with 10-tab dispatch, help overlay toggle, console input mode, j/k scroll, number-key tab switching, profiler event/duration controls. |
| `action.rs` | ✅ Complete | Action enum message bus (Tick, Render, Quit, TabNext/Prev, DataRefreshed, scroll, filter, resize, alerts). |
| `collector.rs` | ✅ Complete | AppState with ArcSwap + watch channel + Mutex history. 4 async tasks: Actuator (2s), Jolokia (3s), thread dump (10s), GC log tailer (500ms). Snapshot merging, DerivedMetrics computation (GC throughput, HTTP RPS, allocation rate). |
| `config.rs` | ✅ Complete | Figment-based config loading (compiled defaults → /etc → ~/.config → ./drishti-jvm.toml → DRISHTI_ env vars). All thresholds tunable. CLI overrides. 1 test. |
| `profiler.rs` | ✅ Complete | ProfileManager with async-profiler integration (Jolokia exec + local asprof CLI). 4 event types (CPU/alloc/wall/lock). Browser SVG output. collapsed_to_tree() parser for TUI rendering. 4 tests. |
| `components/header.rs` | ✅ Complete | Live status bar showing connection indicator, VM name, uptime, GC algorithm, readonly flag. |
| `components/footer.rs` | ✅ Complete | Context-sensitive keybinding hints. |
| `components/help.rs` | ✅ Complete | Full keybinding overlay on `?` key. |
| `tabs/overview.rs` | ✅ Complete | Heap gauge, CPU gauge, thread summary (with deadlock banner), GC with throughput from DerivedMetrics, HTTP with RPS, HikariCP utilization, live alert feed from AnomalyEngine. |
| `tabs/memory.rs` | ✅ Complete | Per-pool table, heap/non-heap gauges, GC collector stats table. |
| `tabs/threads.rs` | ✅ Complete | Thread state bar chart, deadlock banner (red), scrollable thread list with j/k navigation, state-colored rows. |
| `tabs/http.rs` | ✅ Complete | Summary with derived RPS, scrollable endpoint table sorted by count, color-coded by latency/error severity. |
| `tabs/db.rs` | ✅ Complete | HikariCP gauge with saturation detection, Tomcat thread pool gauge, class loading stats. |
| `tabs/logs.rs` | ✅ Complete | 2000-entry log buffer, color-coded severity, level filter cycling (L key), auto-scroll mode, error/warn counters. |
| `tabs/mbeans.rs` | ✅ Complete | Split-pane: collapsible domain → MBean tree (left), attribute name/value table (right). j/k navigation, Enter to expand. |
| `tabs/profiler.rs` | ✅ Complete | Event type selector, duration controls, recording status with progress bar, async-profiler instructions. |
| `tabs/console.rs` | ✅ Complete | Arthas-style REPL with 9 commands (dashboard, threads, gc, memory, heap, uptime, alerts, clear, help). Command history ↑/↓, cursor left/right, backspace. Color-coded output. |
| `tabs/recommendations.rs` | ✅ Complete | Split-pane: anomaly alerts (top), tuning recommendations with JVM flags (bottom). |

### 2.6 Infrastructure

| Item | Status | Description |
|------|--------|-------------|
| Docker lab | ✅ Complete | docker-compose.yml + Dockerfile.petclinic (Petclinic + Jolokia + full Actuator + G1GC logging + HikariCP MBeans + load generator). |
| verify-sources.sh | ✅ Complete | 9-section verification of all data endpoints with fixture saving. |
| stress-test.sh | ✅ Complete | 5 scenarios (gc-pressure, high-load, slow-requests, error-spike, mixed). |
| api-cheatsheet.sh | ✅ Complete | Runnable cheatsheet for all Jolokia + Actuator API calls. |
| setup-env.sh | ✅ Complete | Rust toolchain + dev tools + compile test. |
| grab-gclog.sh | ✅ Complete | GC log extraction from Docker container. |
| config/default.toml | ✅ Complete | Full configuration with tunable thresholds. |
| justfile | ✅ Complete | Task runner for common workflows. |
| Architecture DOCX | ✅ Complete | 12-section system design document with diagrams. |

---

## 3. What NEEDS WORK 🔧

### 3.1 Must-Fix Before First Run (Priority: CRITICAL)

These will likely surface when you run `cargo build --workspace`:

| Item | Issue | Fix |
|------|-------|-----|
| Compile errors | Module references, import paths, and trait bounds may have mismatches from iterative file rewrites across sessions | Run `cargo build --workspace 2>&1`, fix each error sequentially. Most will be missing `use` statements or type mismatches. |
| `normalize.rs` references `crate::prometheus::Sample` | The `find_gauge_value` method references types from `prometheus.rs` — needs the import path verified | Check the import in `normalize.rs` matches the actual `Sample` struct location |
| `logfile.rs` missing from collector | Remote log tailer is implemented but not wired into `collector.rs` `spawn_collectors()` | Add a 5th task in `spawn_collectors()` that starts `spawn_remote_log_tailer()` and feeds chunks into `LogsTab` |
| `LogsTab` buffer not fed by collector | `LogsTab` has the buffer and rendering but no data source connected | Wire the remote log tailer output OR the GC log output into the logs tab buffer via a channel |
| `MBeansTab.load_mbeans()` never called | MBean tree is implemented but nobody calls `load_mbeans()` with data from Jolokia search | Add a one-time Jolokia search (`{"type":"search","mbean":"*:*"}`) on startup and call `mbeans.load_mbeans(names)` |
| `ProfilerTab` Enter key not wired to start recording | UI shows controls but pressing Enter doesn't trigger `start_local()` or Jolokia exec | Add Enter handling in `app.rs` for `Tab::Profiler` that calls the appropriate start method |

### 3.2 Should-Fix Before v1.0 (Priority: HIGH)

| Item | Description | Effort |
|------|-------------|--------|
| End-to-end integration test | Connect to real Petclinic, verify all 10 tabs render without panic | 1 day |
| Error recovery in collector | Currently a single Jolokia/Actuator failure logs a warning but doesn't update UI status | 0.5 day |
| Header connection status | Header shows connected/disconnected but doesn't track per-source status (Actuator OK but Jolokia down) | 0.5 day |
| Thread dump → ThreadSummary state_counts sync | Thread dump updates `snap.threads` but doesn't recompute `thread_summary.state_counts` from the actual thread list | 0.5 day |
| `--readonly` enforcement | Flag exists and shows in header but mutating actions (log-level change in console, profiler start) aren't gated | 0.5 day |
| Config actually used by collector | Config is loaded in main.rs but polling intervals, URLs, and thresholds still hardcoded in collector.rs | 1 day |
| Persistence wired into collector | SQLite module exists with insert/query but nobody calls `insert_snapshot()` on each tick | 0.5 day |
| Multi-JVM targets wired into TUI | TargetManager exists with cycling/comparison but no UI for adding targets or switching | 1 day |
| Profiler async execution | `start_local()` spawns asprof but doesn't track completion or update status to Complete | 1 day |

### 3.3 Nice-to-Have for v1.0 (Priority: MEDIUM)

| Item | Description | Effort |
|------|-------------|--------|
| `--once --json` headless mode | Print a single JvmSnapshot as JSON and exit (for scripting) | 0.5 day |
| `--once --recommendations` mode | Print tuning recommendations as text and exit | 0.5 day |
| Cross-compile CI | GitHub Actions for linux-x86_64, linux-aarch64, macOS builds | 0.5 day |
| Binary size optimization | strip, LTO, opt-level=z, verify < 15MB | 0.5 day |
| Adaptive thresholds (Layer 3) | Alert at N× the rolling 30-minute baseline instead of fixed values | 2 days |
| Desktop notifications | `notify-send` on Linux, `osascript` on macOS for critical alerts | 0.5 day |
| Export to CSV/JSON | Save current snapshot or history to file from console | 0.5 day |
| Sparkline widgets | Add sparklines to Overview tab for heap/CPU/GC trends | 1 day |
| MBean write/invoke | MBeans tab shows attributes but can't modify or invoke operations | 1 day |
| Console `loglevel` command | Console REPL command that calls ActuatorClient.set_log_level() | 0.5 day |

### 3.4 Post-v1.0 Roadmap (Priority: LOW)

| Item | Description | Effort |
|------|-------------|--------|
| Kubernetes pod discovery | List pods → auto-connect to Spring Boot services | 2 days |
| Alerting webhooks | Slack/Discord/PagerDuty notifications for critical alerts | 1 day |
| Plugin system | Custom MBean dashboards via TOML definitions | 3 days |
| JFR file parser | Parse JDK Flight Recorder files for offline analysis | 3 days |
| N+1 query detection | Analyze SQL query patterns from log/trace data | 2 days |
| Publish crates | Release `drishti-jolokia` and `drishti-actuator` as standalone crates on crates.io | 1 day |
| TUI themes | Solarized, Dracula, light mode, custom color schemes | 1 day |
| Mouse support | Click on tabs, scroll with mouse wheel, click on table rows | 1 day |
| Comparison view | Side-by-side metrics for two JVM targets | 2 days |
| Historical charts | Load SQLite data into Chart widgets with time range selection | 2 days |

---

## 4. File Inventory (52 Rust files)

```
drishti-jvm/
├── Cargo.toml                                    # Workspace root (7 workspace deps)
├── config/default.toml                           # Default config (all thresholds)
├── docker/
│   ├── Dockerfile.petclinic                      # Lab target (Petclinic + Jolokia)
│   └── docker-compose.yml                        # Lab environment + load generator
├── scripts/
│   ├── setup-env.sh                              # Rust + tools setup
│   ├── verify-sources.sh                         # Data source verification + fixture gen
│   ├── grab-gclog.sh                             # GC log extraction
│   ├── stress-test.sh                            # 5 stress scenarios
│   └── api-cheatsheet.sh                         # Runnable API reference
├── fixtures/
│   └── README.md                                 # Fixture documentation
│
├── crates/drishti-core/                          # 1,991 lines / 9 files
│   ├── src/
│   │   ├── lib.rs                                # Module declarations
│   │   ├── model.rs                              # 12 struct families (JvmSnapshot root)
│   │   ├── timeseries.rs                         # Ring buffer + linear regression
│   │   ├── anomaly.rs                            # AnomalyDetector trait + engine
│   │   ├── detectors.rs                          # 5 detectors (leak, GC, deadlock, pool, heap)
│   │   ├── recommend.rs                          # TuningRule trait + engine
│   │   ├── rules.rs                              # 5 rules (heap, Xms=Xmx, ZGC, Hikari, Tomcat)
│   │   ├── persistence.rs                        # SQLite storage (feature-gated)
│   │   └── targets.rs                            # Multi-JVM TargetManager
│   └── Cargo.toml
│
├── crates/drishti-jolokia/                       # 649 lines / 6 files
│   ├── src/
│   │   ├── lib.rs
│   │   ├── request.rs                            # BulkRequestBuilder (8-element standard)
│   │   ├── response.rs                           # JolokiaResponse envelope
│   │   ├── client.rs                             # JolokiaClient (reqwest + auth)
│   │   └── converter.rs                          # Bulk response → JvmSnapshot
│   ├── tests/
│   │   └── fixture_tests.rs                      # 2 tests against JSON fixture
│   └── Cargo.toml
│
├── crates/drishti-actuator/                      # 1,432 lines / 9 files
│   ├── src/
│   │   ├── lib.rs
│   │   ├── client.rs                             # ActuatorClient (prometheus/health/loggers)
│   │   ├── prometheus.rs                         # Exposition format parser
│   │   ├── converter.rs                          # Prometheus → JvmSnapshot (uses MetricRegistry)
│   │   ├── health.rs                             # Boot 2.x + 3.x health parser
│   │   ├── threads.rs                            # Thread dump → Vec<ThreadInfo>
│   │   ├── normalize.rs                          # MetricRegistry (30+ canonical metrics)
│   │   └── logfile.rs                            # Remote log tailing (HTTP Range)
│   ├── tests/
│   │   ├── fixture_tests.rs                      # 3 tests
│   │   └── fixtures/prometheus.txt               # Realistic prometheus output
│   └── Cargo.toml
│
├── crates/drishti-gclog/                         # 739 lines / 7 files
│   ├── src/
│   │   ├── lib.rs                                # Algorithm detection + types
│   │   ├── parser.rs                             # Unified log prefix parser
│   │   ├── g1.rs                                 # G1GC event parser
│   │   ├── zgc.rs                                # ZGC parser (classic + generational)
│   │   ├── shenandoah.rs                         # Shenandoah 7-phase parser
│   │   └── tailer.rs                             # Async file tailer
│   ├── tests/
│   │   ├── fixture_tests.rs                      # 5 tests
│   │   └── fixtures/g1_sample.log                # G1 log fixture (5 events)
│   └── Cargo.toml
│
└── crates/drishti-tui/                           # 2,717 lines / 21 files
    ├── src/
    │   ├── main.rs                               # CLI + terminal setup + event loop
    │   ├── app.rs                                # 10-tab dispatch + keybindings
    │   ├── action.rs                             # Action message bus
    │   ├── collector.rs                          # 4 async tasks + AppState + DerivedMetrics
    │   ├── config.rs                             # Figment config loading
    │   ├── profiler.rs                           # async-profiler integration
    │   ├── components/
    │   │   ├── mod.rs                            # Component trait
    │   │   ├── header.rs                         # Live status bar
    │   │   ├── footer.rs                         # Keybinding hints
    │   │   └── help.rs                           # ? overlay
    │   └── tabs/
    │       ├── mod.rs                            # Tab enum (10 variants)
    │       ├── overview.rs                       # Gauges + alerts + derived metrics
    │       ├── memory.rs                         # Pool table + GC stats
    │       ├── threads.rs                        # Bar chart + scrollable thread list
    │       ├── http.rs                           # RPS + scrollable endpoint table
    │       ├── db.rs                             # HikariCP + Tomcat + classes
    │       ├── logs.rs                           # Log buffer + level filter
    │       ├── mbeans.rs                         # Tree browser + attribute table
    │       ├── profiler.rs                       # Recording controls + status
    │       ├── console.rs                        # REPL with 9 commands
    │       └── recommendations.rs                # Alerts + tuning rules + JVM flags
    └── Cargo.toml
```

---

## 5. Dependency Graph

```
                    ┌─────────────────┐
                    │   drishti-tui   │  Binary (2,717 lines)
                    └───┬──┬──┬──┬────┘
                        │  │  │  │
          ┌─────────────┘  │  │  └──────────────┐
          │        ┌───────┘  └───────┐         │
          ▼        ▼                  ▼         ▼
  ┌──────────┐ ┌──────────┐  ┌──────────┐ ┌────────┐
  │ drishti- │ │ drishti- │  │ drishti- │ │drishti-│
  │ jolokia  │ │ actuator │  │  gclog   │ │ core   │
  │  649 L   │ │ 1,432 L  │  │  739 L   │ │1,991 L │
  └────┬─────┘ └────┬─────┘  └────┬─────┘ └───▲────┘
       │            │             │            │
       └────────────┴─────────────┴────────────┘
                  all depend on core
```

---

## 6. Getting Started

```bash
# 1. Extract and build
tar xzf drishti-jvm-v0.3.tar.gz
cd drishti-jvm
cargo build --workspace 2>&1 | head -50

# 2. Fix any compile errors (expected on first build — see section 3.1)
# Most will be missing imports or type mismatches

# 3. Run tests
cargo test --workspace

# 4. Start the lab environment
cd docker && docker compose up -d
cd ..

# 5. Verify data sources
./scripts/verify-sources.sh

# 6. Run the TUI
cargo run -p drishti-tui -- \
  --actuator http://localhost:8080/actuator \
  --jolokia http://localhost:8778/jolokia

# 7. Generate GC log fixture
./scripts/grab-gclog.sh

# 8. Run stress tests for interesting metrics
./scripts/stress-test.sh mixed
```

---

## 7. Version History

| Version | Date | Lines | Files | What Changed |
|---------|------|-------|-------|-------------|
| v0.1 | May 2026 | 3,864 | 39 | Core model, collectors, 6 tabs, 5 detectors, 5 rules |
| v0.2 | Jun 2026 | 6,347 | 48 | +Logs tab, +MBeans, +Console REPL, +config, +fixtures, +persistence, +multi-JVM |
| v0.3 | Jul 2026 | 7,528 | 52 | +Metric normalization, +remote log tailing, +async-profiler, +Profiler tab |

---

## 8. Estimated Remaining Effort

| Priority | Items | Total Effort |
|----------|-------|-------------|
| CRITICAL (must-fix) | 6 compile/wiring issues | 1-2 days |
| HIGH (v1.0 quality) | 9 items (error recovery, config wiring, persistence, etc.) | 5-7 days |
| MEDIUM (nice-to-have) | 10 items (headless mode, CI, sparklines, etc.) | 6-8 days |
| LOW (post-v1.0) | 10 items (K8s, webhooks, plugins, JFR, etc.) | 15-20 days |

**Total to v1.0:** ~8-10 working days of focused effort after compile fixes.
**Total to v2.0 (full vision):** ~30 additional working days.
