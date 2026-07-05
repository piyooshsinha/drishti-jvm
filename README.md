# दृष्टि — drishti-jvm

> A Rust + Ratatui TUI that monitors memory, scale, bugs, and performance of Spring Boot / JVM applications — and recommends JVM tuning.

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
cargo run -p drishti-tui -- \
  --actuator http://localhost:8080/actuator \
  --jolokia http://localhost:8778/jolokia
```

## Keybindings

Tab/Shift+Tab: cycle tabs | 1-9, 0: jump to tab | j/k: scroll | ?: help | q: quit

## Workspace — 5 crates, ~7,500 lines

- **drishti-core** — Data model (12 struct families), TimeSeries ring buffer with linear regression, 5 anomaly detectors, 5 tuning rules, SQLite persistence (`--features persistence`), multi-JVM target manager
- **drishti-jolokia** — Jolokia HTTP client with bulk request builder, response parsing, JvmSnapshot converter
- **drishti-actuator** — Spring Boot Actuator client with Prometheus parser, metric-name normalization across Boot 2.x/3.x, health (Boot 2+3), thread dumps, remote log tailing via HTTP Range
- **drishti-gclog** — GC log parsers (G1, ZGC classic + generational, Shenandoah), unified log prefix, algorithm auto-detection, async file tailer
- **drishti-tui** — Ratatui app with 10 tabs (Overview, Memory, Threads, HTTP, DB/Pool, Logs, MBeans, Profiler, Console, Recommendations), async-profiler integration, Arthas-style console REPL

See [PLAN.md](PLAN.md) for the full roadmap and remaining work.

## License

MIT
