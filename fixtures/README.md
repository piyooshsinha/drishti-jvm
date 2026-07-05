# fixtures/

This directory is populated by `scripts/verify-sources.sh`. It contains saved
JSON/text responses from Actuator and Jolokia endpoints, plus GC log samples.

These fixtures serve two purposes:

1. **serde struct development** (Phase 2) — write `#[derive(Deserialize)]` structs
   and test them against real data without needing a running JVM.

2. **Regression tests** — ensure parsers don't break when the API shape changes
   between Spring Boot / Jolokia versions.

## Generated files

| File | Source | Used by |
|------|--------|---------|
| `actuator_index.json` | GET /actuator | endpoint discovery |
| `actuator_health.json` | GET /actuator/health | HealthResponse |
| `actuator_metrics_index.json` | GET /actuator/metrics | metric name list |
| `actuator_prometheus.txt` | GET /actuator/prometheus | PrometheusParser |
| `actuator_threaddump.json` | GET /actuator/threaddump | ThreadInfo |
| `actuator_httpexchanges.json` | GET /actuator/httpexchanges | HttpEndpointMetrics |
| `actuator_loggers.json` | GET /actuator/loggers | LoggerControl |
| `metric_jvm_memory_used.json` | GET /actuator/metrics/jvm.memory.used | MemoryUsage |
| `metric_jvm_gc_pause.json` | GET /actuator/metrics/jvm.gc.pause | GcEvent |
| `metric_hikaricp_active.json` | GET /actuator/metrics/hikaricp.connections.active | HikariMetrics |
| `jolokia_version.json` | GET /jolokia/version | client init |
| `jolokia_bulk.json` | POST /jolokia (8-element bulk) | JvmSnapshot |
| `jolokia_mbean_list.json` | POST /jolokia (search *:*) | MBean browser |
| `jolokia_hikaricp.json` | POST /jolokia (HikariCP MBean) | HikariMetrics |
| `gc_sample.log` | GC log tail (200 lines) | GcLogParser |
| `gc_full.log` | Full GC log | GcLogParser regression |
| `gc_pauses_only.log` | Extracted pause events | GcEvent parsing |

## Regenerating

```bash
# From project root, with target app running:
./scripts/verify-sources.sh
./scripts/grab-gclog.sh
```
