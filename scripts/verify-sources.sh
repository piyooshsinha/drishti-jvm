#!/usr/bin/env bash
##############################################################################
# jvmtui — Phase 0 Data Source Verification & Fixture Generator
#
# Verifies all data endpoints are live and saves responses as test fixtures.
# These fixtures become the ground truth for serde struct development in Phase 2.
#
# Usage:
#   ./scripts/verify-sources.sh
#   ./scripts/verify-sources.sh --host 192.168.1.50    # remote target
#   ./scripts/verify-sources.sh --skip-jolokia          # actuator only
#
# Prerequisites:
#   - curl, jq installed
#   - Target Spring Boot app running with Actuator exposed
#   - Jolokia agent running (unless --skip-jolokia)
##############################################################################

set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────────────

APP_HOST="${APP_HOST:-localhost}"
APP_PORT="${APP_PORT:-8080}"
JOLOKIA_PORT="${JOLOKIA_PORT:-8778}"
FIXTURE_DIR="./fixtures"
SKIP_JOLOKIA=false
TIMEOUT=10

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'  # No Color
BOLD='\033[1m'

# ── Argument parsing ──────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case $1 in
        --host)       APP_HOST="$2"; shift 2 ;;
        --app-port)   APP_PORT="$2"; shift 2 ;;
        --jolokia-port) JOLOKIA_PORT="$2"; shift 2 ;;
        --skip-jolokia) SKIP_JOLOKIA=true; shift ;;
        --help|-h)
            echo "Usage: $0 [--host HOST] [--app-port PORT] [--jolokia-port PORT] [--skip-jolokia]"
            exit 0 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

ACTUATOR_BASE="http://${APP_HOST}:${APP_PORT}/actuator"
JOLOKIA_BASE="http://${APP_HOST}:${JOLOKIA_PORT}/jolokia"

# ── Helpers ────────────────────────────────────────────────────────────────

pass=0
fail=0
warn=0

check_pass() {
    ((pass++))
    echo -e "  ${GREEN}✓ PASS${NC}  $1"
}

check_fail() {
    ((fail++))
    echo -e "  ${RED}✗ FAIL${NC}  $1"
    echo -e "         ${RED}→ $2${NC}"
}

check_warn() {
    ((warn++))
    echo -e "  ${YELLOW}⚠ WARN${NC}  $1"
    echo -e "         ${YELLOW}→ $2${NC}"
}

section() {
    echo ""
    echo -e "${BOLD}${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BOLD}${BLUE}  $1${NC}"
    echo -e "${BOLD}${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

save_fixture() {
    local name="$1"
    local content="$2"
    local ext="${3:-json}"
    echo "$content" > "${FIXTURE_DIR}/${name}.${ext}"
    echo -e "         ${CYAN}→ Saved: fixtures/${name}.${ext}${NC}"
}

fetch() {
    curl -sf --max-time "$TIMEOUT" "$@" 2>/dev/null
}

fetch_or_empty() {
    curl -s --max-time "$TIMEOUT" "$@" 2>/dev/null || echo ""
}

# ── Prerequisites check ───────────────────────────────────────────────────

section "0. Prerequisites Check"

for cmd in curl jq; do
    if command -v "$cmd" &> /dev/null; then
        check_pass "$cmd is installed ($(command -v $cmd))"
    else
        check_fail "$cmd is NOT installed" "Install it: sudo apt install $cmd"
    fi
done

mkdir -p "$FIXTURE_DIR"
check_pass "Fixture directory created: $FIXTURE_DIR"

echo ""
echo -e "  ${CYAN}Target App:  ${APP_HOST}:${APP_PORT}${NC}"
echo -e "  ${CYAN}Jolokia:     ${APP_HOST}:${JOLOKIA_PORT}${NC}"

# ── 1. Actuator Discovery ─────────────────────────────────────────────────

section "1. Actuator Discovery"

ACTUATOR_INDEX=$(fetch_or_empty "$ACTUATOR_BASE")
if [ -n "$ACTUATOR_INDEX" ] && echo "$ACTUATOR_INDEX" | jq -e '._links' > /dev/null 2>&1; then
    ENDPOINTS=$(echo "$ACTUATOR_INDEX" | jq -r '._links | keys[]' | sort)
    ENDPOINT_COUNT=$(echo "$ENDPOINTS" | wc -l)
    check_pass "Actuator index reachable — $ENDPOINT_COUNT endpoints discovered"
    save_fixture "actuator_index" "$ACTUATOR_INDEX"

    # List discovered endpoints
    echo -e "         ${CYAN}Endpoints:${NC}"
    echo "$ENDPOINTS" | while read -r ep; do
        echo -e "           ${CYAN}• $ep${NC}"
    done
else
    check_fail "Actuator index not reachable at $ACTUATOR_BASE" \
        "Ensure spring-boot-starter-actuator is in pom.xml and management.endpoints.web.exposure.include is configured"
fi

# ── 2. Actuator Health ─────────────────────────────────────────────────────

section "2. Actuator Health"

HEALTH=$(fetch_or_empty "$ACTUATOR_BASE/health")
if [ -n "$HEALTH" ]; then
    STATUS=$(echo "$HEALTH" | jq -r '.status // "UNKNOWN"')
    if [ "$STATUS" = "UP" ]; then
        check_pass "Health status: $STATUS"
    else
        check_warn "Health status: $STATUS" "Expected UP"
    fi

    # Check for component details (requires show-details=always)
    COMPONENTS=$(echo "$HEALTH" | jq -r '.components // {} | keys[]' 2>/dev/null | tr '\n' ', ' | sed 's/,$//')
    if [ -n "$COMPONENTS" ]; then
        check_pass "Health components visible: $COMPONENTS"
    else
        check_warn "No health components visible" \
            "Add management.endpoint.health.show-details=always to application.properties"
    fi

    save_fixture "actuator_health" "$(echo "$HEALTH" | jq .)"
else
    check_fail "Health endpoint not reachable" "GET $ACTUATOR_BASE/health failed"
fi

# ── 3. Actuator Metrics ───────────────────────────────────────────────────

section "3. Actuator Metrics"

METRICS_INDEX=$(fetch_or_empty "$ACTUATOR_BASE/metrics")
if [ -n "$METRICS_INDEX" ]; then
    METRIC_COUNT=$(echo "$METRICS_INDEX" | jq -r '.names | length')
    check_pass "Metrics endpoint live — $METRIC_COUNT metric names available"
    save_fixture "actuator_metrics_index" "$(echo "$METRICS_INDEX" | jq .)"

    # Check critical JVM metrics exist
    CRITICAL_METRICS=(
        "jvm.memory.used"
        "jvm.memory.max"
        "jvm.gc.pause"
        "jvm.threads.live"
        "process.cpu.usage"
        "system.cpu.usage"
        "http.server.requests"
    )

    for metric in "${CRITICAL_METRICS[@]}"; do
        if echo "$METRICS_INDEX" | jq -r '.names[]' | grep -qx "$metric"; then
            # Fetch the actual metric detail and save it
            DETAIL=$(fetch_or_empty "$ACTUATOR_BASE/metrics/$metric")
            if [ -n "$DETAIL" ]; then
                check_pass "Metric available: $metric"
                SAFE_NAME=$(echo "$metric" | tr '.' '_')
                save_fixture "metric_${SAFE_NAME}" "$(echo "$DETAIL" | jq .)"
            fi
        else
            check_warn "Metric missing: $metric" "May need micrometer-registry-prometheus dependency"
        fi
    done

    # Check HikariCP metrics
    HIKARI_METRICS=$(echo "$METRICS_INDEX" | jq -r '.names[]' | grep -c "hikaricp" || true)
    if [ "$HIKARI_METRICS" -gt 0 ]; then
        check_pass "HikariCP metrics present ($HIKARI_METRICS metrics)"
        # Save one representative HikariCP metric
        HIKARI_DETAIL=$(fetch_or_empty "$ACTUATOR_BASE/metrics/hikaricp.connections.active")
        [ -n "$HIKARI_DETAIL" ] && save_fixture "metric_hikaricp_active" "$(echo "$HIKARI_DETAIL" | jq .)"
    else
        check_warn "No HikariCP metrics found" \
            "Add spring.datasource.hikari.register-mbeans=true to application.properties"
    fi

    # Check Tomcat metrics
    TOMCAT_METRICS=$(echo "$METRICS_INDEX" | jq -r '.names[]' | grep -c "tomcat" || true)
    if [ "$TOMCAT_METRICS" -gt 0 ]; then
        check_pass "Tomcat metrics present ($TOMCAT_METRICS metrics)"
    else
        check_warn "No Tomcat metrics found" \
            "Add server.tomcat.mbeanregistry.enabled=true to application.properties"
    fi
else
    check_fail "Metrics endpoint not reachable" "GET $ACTUATOR_BASE/metrics failed"
fi

# ── 4. Prometheus Scrape ──────────────────────────────────────────────────

section "4. Prometheus Exposition Format"

PROM=$(fetch_or_empty "$ACTUATOR_BASE/prometheus")
if [ -n "$PROM" ]; then
    LINE_COUNT=$(echo "$PROM" | wc -l)
    METRIC_FAMILIES=$(echo "$PROM" | grep '^# TYPE' | wc -l)
    check_pass "Prometheus endpoint live — $LINE_COUNT lines, $METRIC_FAMILIES metric families"
    save_fixture "actuator_prometheus" "$PROM" "txt"

    # Verify key metric families in prometheus format
    for family in "jvm_memory_used_bytes" "jvm_gc_pause_seconds" "jvm_threads" "process_cpu_usage" "http_server_requests_seconds"; do
        if echo "$PROM" | grep -q "^${family}"; then
            check_pass "Prometheus family present: $family"
        elif echo "$PROM" | grep -q "${family}"; then
            check_pass "Prometheus family present: $family (with labels)"
        else
            check_warn "Prometheus family missing: $family" "May be named differently in your Micrometer version"
        fi
    done
else
    check_fail "Prometheus endpoint not reachable" \
        "Add micrometer-registry-prometheus to pom.xml and expose prometheus in management.endpoints.web.exposure.include"
fi

# ── 5. Thread Dump ─────────────────────────────────────────────────────────

section "5. Thread Dump"

THREADS=$(fetch_or_empty "$ACTUATOR_BASE/threaddump")
if [ -n "$THREADS" ]; then
    THREAD_COUNT=$(echo "$THREADS" | jq -r '.threads | length' 2>/dev/null || echo "?")
    check_pass "Thread dump endpoint live — $THREAD_COUNT threads"
    save_fixture "actuator_threaddump" "$(echo "$THREADS" | jq .)"

    # Check thread state distribution
    if [ "$THREAD_COUNT" != "?" ]; then
        echo "$THREADS" | jq -r '.threads[].threadState' | sort | uniq -c | sort -rn | while read count state; do
            echo -e "         ${CYAN}  $state: $count${NC}"
        done
    fi
else
    check_fail "Thread dump endpoint not reachable" "Ensure 'threaddump' is in management.endpoints.web.exposure.include"
fi

# ── 6. HTTP Exchanges ─────────────────────────────────────────────────────

section "6. HTTP Exchanges"

EXCHANGES=$(fetch_or_empty "$ACTUATOR_BASE/httpexchanges")
if [ -n "$EXCHANGES" ]; then
    EXCHANGE_COUNT=$(echo "$EXCHANGES" | jq -r '.exchanges | length' 2>/dev/null || echo "0")
    check_pass "HTTP exchanges endpoint live — $EXCHANGE_COUNT recorded"
    save_fixture "actuator_httpexchanges" "$(echo "$EXCHANGES" | jq .)"
else
    check_warn "HTTP exchanges endpoint not available" \
        "Add httpexchanges to exposure.include and add InMemoryHttpExchangeRepository bean if needed"
fi

# ── 7. Loggers ─────────────────────────────────────────────────────────────

section "7. Loggers (for live log-level control)"

LOGGERS=$(fetch_or_empty "$ACTUATOR_BASE/loggers")
if [ -n "$LOGGERS" ]; then
    LOGGER_COUNT=$(echo "$LOGGERS" | jq -r '.loggers | length' 2>/dev/null || echo "?")
    check_pass "Loggers endpoint live — $LOGGER_COUNT loggers"
    save_fixture "actuator_loggers" "$(echo "$LOGGERS" | jq .)"

    # Test write capability — set and restore a logger level
    echo -e "         ${CYAN}Testing log-level write (set org.springframework to DEBUG, then restore)...${NC}"
    ORIGINAL=$(echo "$LOGGERS" | jq -r '.loggers["org.springframework"].configuredLevel // "null"')

    WRITE_RESULT=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" \
        -X POST "$ACTUATOR_BASE/loggers/org.springframework" \
        -H "Content-Type: application/json" \
        -d '{"configuredLevel":"DEBUG"}' 2>/dev/null || echo "000")

    if [ "$WRITE_RESULT" = "204" ] || [ "$WRITE_RESULT" = "200" ]; then
        check_pass "Logger level change works (HTTP $WRITE_RESULT)"
        # Restore
        if [ "$ORIGINAL" = "null" ]; then
            curl -s -X POST "$ACTUATOR_BASE/loggers/org.springframework" \
                -H "Content-Type: application/json" \
                -d '{"configuredLevel":null}' > /dev/null 2>&1
        else
            curl -s -X POST "$ACTUATOR_BASE/loggers/org.springframework" \
                -H "Content-Type: application/json" \
                -d "{\"configuredLevel\":\"$ORIGINAL\"}" > /dev/null 2>&1
        fi
        echo -e "         ${CYAN}Restored to: $ORIGINAL${NC}"
    else
        check_warn "Logger level change returned HTTP $WRITE_RESULT" "May be read-only"
    fi
else
    check_fail "Loggers endpoint not reachable" "Add 'loggers' to management.endpoints.web.exposure.include"
fi

# ── 8. Jolokia ─────────────────────────────────────────────────────────────

if [ "$SKIP_JOLOKIA" = true ]; then
    section "8. Jolokia (SKIPPED — --skip-jolokia)"
else
    section "8. Jolokia Agent"

    # 8a. Version check
    JOLOKIA_VERSION=$(fetch_or_empty "$JOLOKIA_BASE/version")
    if [ -n "$JOLOKIA_VERSION" ]; then
        AGENT_VER=$(echo "$JOLOKIA_VERSION" | jq -r '.value.agent // "unknown"')
        PROTOCOL=$(echo "$JOLOKIA_VERSION" | jq -r '.value.protocol // "unknown"')
        check_pass "Jolokia agent reachable — v$AGENT_VER (protocol $PROTOCOL)"
        save_fixture "jolokia_version" "$(echo "$JOLOKIA_VERSION" | jq .)"
    else
        check_fail "Jolokia not reachable at $JOLOKIA_BASE" \
            "Ensure -javaagent:jolokia-agent-jvm.jar=port=8778,host=0.0.0.0 is in JAVA_OPTS"
    fi

    # 8b. Bulk read — the core jvmtui polling request
    BULK_REQUEST='[
        {"type":"read","mbean":"java.lang:type=Memory","attribute":["HeapMemoryUsage","NonHeapMemoryUsage"]},
        {"type":"read","mbean":"java.lang:type=Threading","attribute":["ThreadCount","DaemonThreadCount","PeakThreadCount"]},
        {"type":"read","mbean":"java.lang:type=GarbageCollector,name=*","attribute":["CollectionCount","CollectionTime","LastGcInfo"]},
        {"type":"read","mbean":"java.lang:type=MemoryPool,name=*","attribute":["Usage","CollectionUsage","Type"]},
        {"type":"read","mbean":"java.lang:type=OperatingSystem","attribute":["ProcessCpuLoad","SystemCpuLoad","AvailableProcessors","TotalPhysicalMemorySize","FreePhysicalMemorySize","SystemLoadAverage"]},
        {"type":"read","mbean":"java.lang:type=Runtime","attribute":["Uptime","VmName","VmVendor","VmVersion","SpecVersion","InputArguments"]},
        {"type":"read","mbean":"java.lang:type=ClassLoading","attribute":["LoadedClassCount","TotalLoadedClassCount","UnloadedClassCount"]},
        {"type":"exec","mbean":"java.lang:type=Threading","operation":"findDeadlockedThreads"}
    ]'

    BULK_RESPONSE=$(fetch_or_empty -X POST "$JOLOKIA_BASE" \
        -H "Content-Type: application/json" \
        -d "$BULK_REQUEST")

    if [ -n "$BULK_RESPONSE" ]; then
        RESPONSE_COUNT=$(echo "$BULK_RESPONSE" | jq 'length')
        OK_COUNT=$(echo "$BULK_RESPONSE" | jq '[.[] | select(.status == 200)] | length')
        ERR_COUNT=$(echo "$BULK_RESPONSE" | jq '[.[] | select(.status != 200)] | length')

        if [ "$ERR_COUNT" -eq 0 ]; then
            check_pass "Bulk read: all $RESPONSE_COUNT responses OK"
        else
            check_warn "Bulk read: $OK_COUNT OK, $ERR_COUNT errors" "Some MBeans may not be available"
        fi

        save_fixture "jolokia_bulk" "$(echo "$BULK_RESPONSE" | jq .)"

        # Extract and display key values
        echo ""
        echo -e "         ${CYAN}── Snapshot from Jolokia ──${NC}"

        # Heap
        HEAP_USED=$(echo "$BULK_RESPONSE" | jq -r '.[0].value.HeapMemoryUsage.used // "?"')
        HEAP_MAX=$(echo "$BULK_RESPONSE" | jq -r '.[0].value.HeapMemoryUsage.max // "?"')
        if [ "$HEAP_USED" != "?" ] && [ "$HEAP_MAX" != "?" ]; then
            HEAP_MB=$((HEAP_USED / 1048576))
            HEAP_MAX_MB=$((HEAP_MAX / 1048576))
            HEAP_PCT=$((HEAP_USED * 100 / HEAP_MAX))
            echo -e "         ${CYAN}  Heap: ${HEAP_MB}M / ${HEAP_MAX_MB}M (${HEAP_PCT}%)${NC}"
        fi

        # Threads
        THREADS=$(echo "$BULK_RESPONSE" | jq -r '.[1].value.ThreadCount // "?"')
        echo -e "         ${CYAN}  Threads: $THREADS${NC}"

        # CPU
        PROC_CPU=$(echo "$BULK_RESPONSE" | jq -r '.[4].value.ProcessCpuLoad // "?"')
        if [ "$PROC_CPU" != "?" ]; then
            CPU_PCT=$(echo "$PROC_CPU" | awk '{printf "%.1f", $1 * 100}')
            echo -e "         ${CYAN}  Process CPU: ${CPU_PCT}%${NC}"
        fi

        # Uptime
        UPTIME_MS=$(echo "$BULK_RESPONSE" | jq -r '.[5].value.Uptime // "?"')
        if [ "$UPTIME_MS" != "?" ]; then
            UPTIME_SEC=$((UPTIME_MS / 1000))
            UPTIME_MIN=$((UPTIME_SEC / 60))
            echo -e "         ${CYAN}  Uptime: ${UPTIME_MIN} minutes${NC}"
        fi

        # JVM Version
        VM_NAME=$(echo "$BULK_RESPONSE" | jq -r '.[5].value.VmName // "?"')
        VM_VER=$(echo "$BULK_RESPONSE" | jq -r '.[5].value.VmVersion // "?"')
        echo -e "         ${CYAN}  VM: $VM_NAME $VM_VER${NC}"

        # Deadlocks
        DEADLOCKS=$(echo "$BULK_RESPONSE" | jq -r '.[7].value // "null"')
        if [ "$DEADLOCKS" = "null" ]; then
            echo -e "         ${GREEN}  Deadlocks: none${NC}"
        else
            echo -e "         ${RED}  DEADLOCKS DETECTED: $DEADLOCKS${NC}"
        fi
    else
        check_fail "Jolokia bulk read failed" "POST $JOLOKIA_BASE with bulk request returned nothing"
    fi

    # 8c. MBean search — verify we can discover all MBeans
    MBEAN_LIST=$(fetch_or_empty -X POST "$JOLOKIA_BASE" \
        -H "Content-Type: application/json" \
        -d '{"type":"search","mbean":"*:*"}')

    if [ -n "$MBEAN_LIST" ]; then
        MBEAN_COUNT=$(echo "$MBEAN_LIST" | jq -r '.value | length' 2>/dev/null || echo "?")
        check_pass "MBean search: $MBEAN_COUNT MBeans discoverable"
        save_fixture "jolokia_mbean_list" "$(echo "$MBEAN_LIST" | jq .)"
    fi

    # 8d. HikariCP via Jolokia (if available)
    HIKARI_JOLOKIA=$(fetch_or_empty -X POST "$JOLOKIA_BASE" \
        -H "Content-Type: application/json" \
        -d '{"type":"read","mbean":"com.zaxxer.hikari:type=Pool (PetclinicPool)"}')

    if [ -n "$HIKARI_JOLOKIA" ] && echo "$HIKARI_JOLOKIA" | jq -e '.status == 200' > /dev/null 2>&1; then
        check_pass "HikariCP MBean accessible via Jolokia"
        save_fixture "jolokia_hikaricp" "$(echo "$HIKARI_JOLOKIA" | jq .)"
    else
        check_warn "HikariCP MBean not found via Jolokia" \
            "Ensure spring.datasource.hikari.register-mbeans=true and pool-name matches"
    fi
fi

# ── 9. GC Log ──────────────────────────────────────────────────────────────

section "9. GC Log"

# Try common locations
GC_LOG_PATHS=(
    "./gc.log"
    "../spring-petclinic/gc.log"
    "/tmp/gc.log"
)

GC_LOG_FOUND=""
for path in "${GC_LOG_PATHS[@]}"; do
    if [ -f "$path" ]; then
        GC_LOG_FOUND="$path"
        break
    fi
done

if [ -n "$GC_LOG_FOUND" ]; then
    GC_LINES=$(wc -l < "$GC_LOG_FOUND")
    GC_SIZE=$(du -h "$GC_LOG_FOUND" | cut -f1)
    check_pass "GC log found: $GC_LOG_FOUND ($GC_LINES lines, $GC_SIZE)"

    # Detect collector type
    if grep -q "G1 Evacuation Pause\|G1 Humongous Allocation\|Pause Young (Normal)" "$GC_LOG_FOUND"; then
        check_pass "Collector detected: G1GC"
    elif grep -q "Garbage Collection (.*)" "$GC_LOG_FOUND" | head -1 | grep -q "ZGC\|Z:"; then
        check_pass "Collector detected: ZGC"
    elif grep -q "Pause Init Mark\|Shenandoah" "$GC_LOG_FOUND"; then
        check_pass "Collector detected: Shenandoah"
    else
        check_warn "Could not auto-detect GC collector type" "Check GC log format"
    fi

    # Save last 200 lines as fixture
    tail -200 "$GC_LOG_FOUND" > "${FIXTURE_DIR}/gc_sample.log"
    echo -e "         ${CYAN}→ Saved: fixtures/gc_sample.log (last 200 lines)${NC}"

    # Show sample
    echo -e "         ${CYAN}── Last 3 GC events ──${NC}"
    grep -E "Pause (Young|Full|Mixed|Init Mark|Final Mark)" "$GC_LOG_FOUND" | tail -3 | while read line; do
        echo -e "         ${CYAN}  $line${NC}"
    done
else
    check_warn "No GC log found" \
        "Add -Xlog:gc*,safepoint:file=./gc.log:time,uptime,level,tags to JAVA_OPTS"
    echo -e "         ${CYAN}Searched: ${GC_LOG_PATHS[*]}${NC}"
    echo -e "         ${CYAN}For Docker: docker compose exec petclinic tail -50 /app/gc.log > fixtures/gc_sample.log${NC}"
fi

# ── Summary ────────────────────────────────────────────────────────────────

section "VERIFICATION SUMMARY"

echo ""
echo -e "  ${GREEN}Passed:  $pass${NC}"
echo -e "  ${YELLOW}Warnings: $warn${NC}"
echo -e "  ${RED}Failed:  $fail${NC}"
echo ""

if [ "$fail" -eq 0 ]; then
    echo -e "  ${GREEN}${BOLD}All critical checks passed! Ready for Phase 1.${NC}"
else
    echo -e "  ${RED}${BOLD}$fail critical check(s) failed. Fix before proceeding.${NC}"
fi

echo ""
echo -e "  ${CYAN}Fixtures saved to: $FIXTURE_DIR/${NC}"
ls -la "$FIXTURE_DIR/" 2>/dev/null | tail -n +2 | while read line; do
    echo -e "    ${CYAN}$line${NC}"
done

echo ""
echo -e "${BOLD}Next steps:${NC}"
echo -e "  1. Review fixtures/ — these become your serde test data in Phase 2"
echo -e "  2. Fix any warnings above for best observability coverage"
echo -e "  3. Run: ${CYAN}cargo init jvmtui${NC} to start Phase 1 workspace scaffolding"
echo ""
