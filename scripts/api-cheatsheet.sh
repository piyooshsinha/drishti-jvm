#!/usr/bin/env bash
##############################################################################
# jvmtui — API Cheatsheet / Interactive Explorer
#
# A runnable cheatsheet: execute any section to see live data from your target.
# Also serves as documentation for every API call jvmtui will make.
#
# Usage:
#   ./scripts/api-cheatsheet.sh                  # show all commands
#   ./scripts/api-cheatsheet.sh heap             # run heap section
#   ./scripts/api-cheatsheet.sh gc               # run GC section
#   ./scripts/api-cheatsheet.sh all              # run everything
##############################################################################

set -euo pipefail

HOST="${APP_HOST:-localhost}"
ACTUATOR="http://${HOST}:8080/actuator"
JOLOKIA="http://${HOST}:8778/jolokia"

CYAN='\033[0;36m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

cmd() {
    echo -e "\n${YELLOW}$ $1${NC}"
    eval "$1" 2>/dev/null | head -60 || echo "(failed — is target running?)"
}

section() {
    echo -e "\n${BOLD}${CYAN}═══════════════════════════════════════════════════${NC}"
    echo -e "${BOLD}${CYAN}  $1${NC}"
    echo -e "${BOLD}${CYAN}═══════════════════════════════════════════════════${NC}"
}

run_section() {
    case "$1" in

    heap|memory)
        section "HEAP & MEMORY"

        echo -e "\n${BOLD}Via Actuator (/metrics):${NC}"
        cmd "curl -s $ACTUATOR/metrics/jvm.memory.used | jq '{name: .name, heap_used_bytes: (.measurements[] | select(.statistic==\"VALUE\") | .value)}'"
        cmd "curl -s $ACTUATOR/metrics/jvm.memory.max | jq '.measurements'"
        cmd "curl -s '$ACTUATOR/metrics/jvm.memory.used?tag=area:heap' | jq '.measurements'"
        cmd "curl -s '$ACTUATOR/metrics/jvm.memory.used?tag=area:nonheap' | jq '.measurements'"

        echo -e "\n${BOLD}Via Actuator (Prometheus — one-shot bulk):${NC}"
        cmd "curl -s $ACTUATOR/prometheus | grep '^jvm_memory_used_bytes'"

        echo -e "\n${BOLD}Via Jolokia (structured, with pool breakdown):${NC}"
        cmd "curl -s -X POST $JOLOKIA -H 'Content-Type: application/json' -d '{\"type\":\"read\",\"mbean\":\"java.lang:type=Memory\",\"attribute\":[\"HeapMemoryUsage\",\"NonHeapMemoryUsage\"]}' | jq '.value'"
        cmd "curl -s -X POST $JOLOKIA -H 'Content-Type: application/json' -d '{\"type\":\"read\",\"mbean\":\"java.lang:type=MemoryPool,name=*\",\"attribute\":[\"Usage\",\"Type\"]}' | jq '.value | to_entries[] | {pool: .key, type: .value.Type, used_mb: (.value.Usage.used / 1048576 | floor), max_mb: (.value.Usage.max / 1048576 | floor)}'"

        echo -e "\n${BOLD}NIO Direct Buffers:${NC}"
        cmd "curl -s $ACTUATOR/prometheus | grep '^jvm_buffer'"
        ;;

    gc)
        section "GARBAGE COLLECTION"

        echo -e "\n${BOLD}Via Actuator:${NC}"
        cmd "curl -s $ACTUATOR/metrics/jvm.gc.pause | jq ."
        cmd "curl -s '$ACTUATOR/metrics/jvm.gc.pause?tag=cause:G1%20Evacuation%20Pause' | jq '.measurements'"
        cmd "curl -s $ACTUATOR/metrics/jvm.gc.memory.allocated | jq '.measurements'"
        cmd "curl -s $ACTUATOR/metrics/jvm.gc.memory.promoted | jq '.measurements'"

        echo -e "\n${BOLD}Via Prometheus:${NC}"
        cmd "curl -s $ACTUATOR/prometheus | grep '^jvm_gc_'"

        echo -e "\n${BOLD}Via Jolokia (with LastGcInfo detail):${NC}"
        cmd "curl -s -X POST $JOLOKIA -H 'Content-Type: application/json' -d '{\"type\":\"read\",\"mbean\":\"java.lang:type=GarbageCollector,name=*\",\"attribute\":[\"CollectionCount\",\"CollectionTime\",\"LastGcInfo\"]}' | jq '.value | to_entries[] | {collector: .key, count: .value.CollectionCount, total_ms: .value.CollectionTime}'"
        ;;

    threads)
        section "THREADS"

        echo -e "\n${BOLD}Via Actuator:${NC}"
        cmd "curl -s $ACTUATOR/metrics/jvm.threads.live | jq '.measurements'"
        cmd "curl -s $ACTUATOR/metrics/jvm.threads.peak | jq '.measurements'"
        cmd "curl -s $ACTUATOR/prometheus | grep '^jvm_threads'"

        echo -e "\n${BOLD}Thread dump summary:${NC}"
        cmd "curl -s $ACTUATOR/threaddump | jq '.threads | group_by(.threadState) | map({state: .[0].threadState, count: length}) | sort_by(-.count)'"

        echo -e "\n${BOLD}Via Jolokia:${NC}"
        cmd "curl -s -X POST $JOLOKIA -H 'Content-Type: application/json' -d '{\"type\":\"read\",\"mbean\":\"java.lang:type=Threading\",\"attribute\":[\"ThreadCount\",\"DaemonThreadCount\",\"PeakThreadCount\"]}' | jq '.value'"

        echo -e "\n${BOLD}Deadlock check:${NC}"
        cmd "curl -s -X POST $JOLOKIA -H 'Content-Type: application/json' -d '{\"type\":\"exec\",\"mbean\":\"java.lang:type=Threading\",\"operation\":\"findDeadlockedThreads\"}' | jq '.value // \"No deadlocks\"'"
        ;;

    cpu)
        section "CPU"

        cmd "curl -s $ACTUATOR/metrics/process.cpu.usage | jq '.measurements'"
        cmd "curl -s $ACTUATOR/metrics/system.cpu.usage | jq '.measurements'"
        cmd "curl -s $ACTUATOR/metrics/system.load.average.1m | jq '.measurements'"

        echo -e "\n${BOLD}Via Jolokia:${NC}"
        cmd "curl -s -X POST $JOLOKIA -H 'Content-Type: application/json' -d '{\"type\":\"read\",\"mbean\":\"java.lang:type=OperatingSystem\",\"attribute\":[\"ProcessCpuLoad\",\"SystemCpuLoad\",\"AvailableProcessors\",\"SystemLoadAverage\"]}' | jq '.value | {process_cpu_pct: (.ProcessCpuLoad * 100 | floor), system_cpu_pct: (.SystemCpuLoad * 100 | floor), cores: .AvailableProcessors, load_avg: .SystemLoadAverage}'"
        ;;

    http)
        section "HTTP METRICS"

        cmd "curl -s $ACTUATOR/metrics/http.server.requests | jq ."
        cmd "curl -s '$ACTUATOR/metrics/http.server.requests?tag=uri:/owners' | jq '.measurements'"
        cmd "curl -s '$ACTUATOR/metrics/http.server.requests?tag=status:500' | jq '.measurements'"

        echo -e "\n${BOLD}Via Prometheus (all URI/method/status combos):${NC}"
        cmd "curl -s $ACTUATOR/prometheus | grep '^http_server_requests_seconds' | head -20"

        echo -e "\n${BOLD}Recent HTTP exchanges:${NC}"
        cmd "curl -s $ACTUATOR/httpexchanges | jq '.exchanges[-3:][] | {method: .request.method, uri: .request.uri, status: .response.status, time_ms: .timeTaken}'"
        ;;

    hikari|db)
        section "HIKARICP / DATABASE"

        cmd "curl -s $ACTUATOR/prometheus | grep '^hikaricp'"
        cmd "curl -s $ACTUATOR/metrics/hikaricp.connections.active | jq '.measurements'"
        cmd "curl -s $ACTUATOR/metrics/hikaricp.connections.idle | jq '.measurements'"
        cmd "curl -s $ACTUATOR/metrics/hikaricp.connections.pending | jq '.measurements'"
        cmd "curl -s $ACTUATOR/metrics/hikaricp.connections.max | jq '.measurements'"

        echo -e "\n${BOLD}Via Jolokia (if MBeans registered):${NC}"
        cmd "curl -s -X POST $JOLOKIA -H 'Content-Type: application/json' -d '{\"type\":\"search\",\"mbean\":\"com.zaxxer.hikari:*\"}' | jq '.value'"
        ;;

    health)
        section "HEALTH & INFO"

        cmd "curl -s $ACTUATOR/health | jq ."
        cmd "curl -s $ACTUATOR/info | jq ."

        echo -e "\n${BOLD}JVM Runtime info via Jolokia:${NC}"
        cmd "curl -s -X POST $JOLOKIA -H 'Content-Type: application/json' -d '{\"type\":\"read\",\"mbean\":\"java.lang:type=Runtime\",\"attribute\":[\"Uptime\",\"VmName\",\"VmVersion\",\"SpecVersion\",\"InputArguments\"]}' | jq '.value | {uptime_minutes: (.Uptime / 60000 | floor), vm: .VmName, version: .VmVersion, spec: .SpecVersion, args: (.InputArguments | map(select(startswith(\"-X\") or startswith(\"-javaagent\"))))}'"
        ;;

    loggers)
        section "LOGGERS (live log-level control)"

        echo -e "\n${BOLD}List configured loggers:${NC}"
        cmd "curl -s $ACTUATOR/loggers | jq '.loggers | to_entries | map(select(.value.configuredLevel != null)) | from_entries'"

        echo -e "\n${BOLD}Check a specific logger:${NC}"
        cmd "curl -s $ACTUATOR/loggers/org.springframework | jq ."

        echo -e "\n${BOLD}Change log level (POST):${NC}"
        echo -e "  curl -X POST $ACTUATOR/loggers/com.example -H 'Content-Type: application/json' -d '{\"configuredLevel\":\"DEBUG\"}'"
        echo -e "  (Returns HTTP 204 on success)"
        ;;

    classes)
        section "CLASS LOADING"

        cmd "curl -s $ACTUATOR/prometheus | grep '^jvm_classes'"
        cmd "curl -s -X POST $JOLOKIA -H 'Content-Type: application/json' -d '{\"type\":\"read\",\"mbean\":\"java.lang:type=ClassLoading\"}' | jq '.value'"
        ;;

    bulk)
        section "JOLOKIA BULK REQUEST (the core jvmtui poll)"
        echo -e "  This is the single HTTP POST that captures ~80% of dashboard state\n"

        cmd "curl -s -X POST $JOLOKIA -H 'Content-Type: application/json' -d '[
  {\"type\":\"read\",\"mbean\":\"java.lang:type=Memory\",\"attribute\":[\"HeapMemoryUsage\",\"NonHeapMemoryUsage\"]},
  {\"type\":\"read\",\"mbean\":\"java.lang:type=Threading\",\"attribute\":[\"ThreadCount\",\"DaemonThreadCount\",\"PeakThreadCount\"]},
  {\"type\":\"read\",\"mbean\":\"java.lang:type=GarbageCollector,name=*\",\"attribute\":[\"CollectionCount\",\"CollectionTime\"]},
  {\"type\":\"read\",\"mbean\":\"java.lang:type=MemoryPool,name=*\",\"attribute\":[\"Usage\",\"Type\"]},
  {\"type\":\"read\",\"mbean\":\"java.lang:type=OperatingSystem\",\"attribute\":[\"ProcessCpuLoad\",\"SystemCpuLoad\",\"AvailableProcessors\"]},
  {\"type\":\"read\",\"mbean\":\"java.lang:type=Runtime\",\"attribute\":[\"Uptime\",\"VmName\",\"VmVersion\"]},
  {\"type\":\"read\",\"mbean\":\"java.lang:type=ClassLoading\"},
  {\"type\":\"exec\",\"mbean\":\"java.lang:type=Threading\",\"operation\":\"findDeadlockedThreads\"}
]' | jq '[.[] | {mbean: .request.mbean, status: .status}]'"
        ;;

    all)
        for s in health heap gc threads cpu http hikari classes loggers bulk; do
            run_section "$s"
        done
        ;;

    *)
        echo "Usage: $0 <section>"
        echo ""
        echo "Sections:"
        echo "  heap       Heap & memory pool metrics"
        echo "  gc         Garbage collection metrics"
        echo "  threads    Thread counts, states, deadlocks"
        echo "  cpu        Process & system CPU"
        echo "  http       HTTP request metrics & exchanges"
        echo "  hikari     HikariCP connection pool"
        echo "  health     Health & JVM runtime info"
        echo "  loggers    Logger levels (read & write)"
        echo "  classes    Class loading stats"
        echo "  bulk       The core Jolokia bulk request"
        echo "  all        Run all sections"
        ;;
    esac
}

run_section "${1:-help}"
