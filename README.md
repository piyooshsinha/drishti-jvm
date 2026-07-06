# दृष्टि — drishti-jvm

[![CI](https://github.com/piyooshsinha/drishti-jvm/actions/workflows/ci.yml/badge.svg)](https://github.com/piyooshsinha/drishti-jvm/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-2021-orange.svg)

> A Rust + Ratatui TUI that monitors memory, scale, bugs, and performance of Spring Boot / JVM applications — and recommends tuning across the whole stack: JVM flags *and* application config.

![Overview tab — heap/CPU gauges, GC throughput, HTTP rate, connection pool, and live anomaly alerts](docs/ui-overview.svg)

## Install

Prebuilt binaries for Linux (x86_64) and macOS (x86_64/arm64) are attached to
[GitHub releases](https://github.com/piyooshsinha/drishti-jvm/releases).
Or build from source: `cargo install --git https://github.com/piyooshsinha/drishti-jvm drishti-jvm`

## Quick Start

```bash
# 1. Setup dev environment
./scripts/setup-env.sh

# 2. Launch the lab Spring Boot app
cd docker && docker compose up -d

# 3. Verify data sources & generate test fixtures
cd .. && ./scripts/verify-sources.sh

# 4. Build and run
cargo build --workspace
cargo run -p drishti-jvm -- \
  --actuator http://localhost:8080/actuator \
  --jolokia http://localhost:8778/jolokia
```

## What it tunes

Every recommendation is a copy-pasteable change — a JVM flag or an `application.properties` line — with the observed evidence and a confidence score. 12 rules across two layers:

| Layer | Rules |
|-------|-------|
| **JVM** | Heap sizing from post-GC occupancy, `-Xms`=`-Xmx`, GC algorithm selection (ZGC for large heaps with long pauses) |
| **Application** | HikariCP pool sizing (cores×2+1) *and* DB-side-bottleneck detection (waiters with pool headroom), Tomcat worker threads (Little's Law) and connection limits, task-executor backlog & oversizing (`spring.task.execution.pool.*`), cache hit-ratio / thrash analysis (`spring.cache.caffeine.spec`), hot read-only endpoints as cache candidates, DEBUG/TRACE log-volume control |

Plus 5 anomaly detectors: memory-leak regression on old-gen (slope + R²), GC pressure, deadlocks, pool exhaustion, and high-heap thresholds.

## Screens

10 tabs: Overview, Memory, Threads, HTTP, DB/Pool, Logs, MBeans, Profiler, Console, Recommend.

**Recommend** — anomaly alerts paired with tuning changes across both layers:

![Recommendations tab — alert table and tuning recommendations with JVM flags and application config](docs/ui-recommendations.svg)

**Console** — Arthas-style REPL with command history:

![Console tab — REPL with dashboard, gc, and alerts commands](docs/ui-console.svg)

*(Illustrative renders of the TUI layout — run it against the Docker lab below for the real thing.)*

## Headless mode (scripting / CI)

```bash
drishti-jvm --once --json               # one JvmSnapshot as pretty JSON
drishti-jvm --once --recommendations    # anomaly alerts + tuning flags as text
drishti-jvm --once                      # compact human-readable summary
```

Exit code is non-zero if no data source is reachable, so it doubles as a health probe.

## Persistence

Build with `--features persistence` and pass `--db metrics.db` to record a snapshot
row every 10s into SQLite (72h retention, pruned hourly) so trends survive restarts.

## End-to-end test without Docker

`./scripts/e2e-local.sh` needs only `java`, `curl`, and `jq`: it generates a Spring
Boot app via start.spring.io, runs it with the Jolokia agent and G1 GC logging, and
asserts that a merged snapshot contains real data from both sources.

## Configuration

All URLs, polling intervals, and alert thresholds are configurable. Load order
(later overrides earlier): compiled defaults → `/etc/drishti-jvm/config.toml` →
`~/.config/drishti-jvm/config.toml` → `./drishti-jvm.toml` → `DRISHTI_*` env vars →
CLI flags. See [config/default.toml](config/default.toml) for every knob.

## Keybindings

Tab/Shift+Tab: cycle tabs | 1-9, 0: jump to tab | j/k: scroll | ?: help | q: quit

## Workspace — 5 crates, ~7,500 lines

- **drishti-core** — Data model (15+ struct families incl. executors, caches, log volume), TimeSeries ring buffer with linear regression, 5 anomaly detectors, 12 tuning rules, SQLite persistence (`--features persistence`), multi-JVM target manager
- **drishti-jolokia** — Jolokia HTTP client with bulk request builder, response parsing, JvmSnapshot converter
- **drishti-actuator** — Spring Boot Actuator client with Prometheus parser, metric-name normalization across Boot 2.x/3.x (JVM, Tomcat, Hikari, executor, cache, logback families), health (Boot 2+3), thread dumps, remote log tailing via HTTP Range
- **drishti-gclog** — GC log parsers (G1, ZGC classic + generational, Shenandoah), unified log prefix, algorithm auto-detection, async file tailer
- **drishti-tui** — Ratatui app with 10 tabs (Overview, Memory, Threads, HTTP, DB/Pool, Logs, MBeans, Profiler, Console, Recommendations), async-profiler integration, Arthas-style console REPL

See [PLAN.md](PLAN.md) for the full roadmap and remaining work.

## License

MIT
