#!/usr/bin/env bash
##############################################################################
# jvmtui — Stress Test Scenarios
#
# Generates various JVM pressure patterns against the target app to produce
# interesting metrics for dashboard development and anomaly detector testing.
#
# Usage:
#   ./scripts/stress-test.sh gc-pressure     # trigger frequent GC
#   ./scripts/stress-test.sh high-load       # saturate HTTP threads
#   ./scripts/stress-test.sh slow-requests   # create latency spikes
#   ./scripts/stress-test.sh mixed           # all patterns combined
#   ./scripts/stress-test.sh stop            # stop all background tests
##############################################################################

set -euo pipefail

APP_HOST="${APP_HOST:-localhost}"
APP_PORT="${APP_PORT:-8080}"
BASE_URL="http://${APP_HOST}:${APP_PORT}"
PID_DIR="/tmp/jvmtui-stress"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

mkdir -p "$PID_DIR"

case "${1:-help}" in

gc-pressure)
    echo -e "${CYAN}▸ GC Pressure: hammering endpoints to spike allocation rate${NC}"
    echo -e "  This should produce frequent young GCs and possibly a full GC"
    echo ""

    # Rapid requests with large query params to allocate strings
    for i in $(seq 1 4); do
        (
            while true; do
                # Generate a large lastName query to force string allocation
                LONG_NAME=$(head -c 200 /dev/urandom | tr -dc 'a-zA-Z' | head -c 100)
                curl -s "${BASE_URL}/owners?lastName=${LONG_NAME}" > /dev/null 2>&1
                # Also hit pages that render lots of objects
                curl -s "${BASE_URL}/owners" > /dev/null 2>&1
                curl -s "${BASE_URL}/vets.html" > /dev/null 2>&1
            done
        ) &
        echo $! >> "$PID_DIR/gc-pressure.pids"
    done

    echo -e "${GREEN}✓ Started 4 worker threads${NC}"
    echo -e "  Watch GC activity: tail -f gc.log | grep 'Pause'"
    echo -e "  Stop with: $0 stop"
    ;;

high-load)
    echo -e "${CYAN}▸ High Load: saturating Tomcat thread pool${NC}"
    echo ""

    if command -v oha &> /dev/null; then
        echo -e "  Using oha for controlled load generation..."
        oha -z 120s -c 50 -q 200 "${BASE_URL}/owners?lastName=" &
        echo $! > "$PID_DIR/high-load.pids"
        echo -e "${GREEN}✓ oha running: 50 concurrent connections, 200 QPS for 2 minutes${NC}"
    else
        echo -e "  Using curl loops (install oha for better load control)..."
        for i in $(seq 1 20); do
            (
                while true; do
                    curl -s "${BASE_URL}/owners?lastName=" > /dev/null 2>&1
                done
            ) &
            echo $! >> "$PID_DIR/high-load.pids"
        done
        echo -e "${GREEN}✓ Started 20 concurrent workers${NC}"
    fi

    echo -e "  Watch thread saturation: curl ${BASE_URL}/actuator/metrics/tomcat.threads.busy"
    echo -e "  Stop with: $0 stop"
    ;;

slow-requests)
    echo -e "${CYAN}▸ Slow Requests: creating latency spikes via search queries${NC}"
    echo ""

    # Mix of fast and slow requests to create bimodal latency
    (
        while true; do
            # Fast request
            curl -s "${BASE_URL}/" > /dev/null 2>&1
            sleep 0.1

            # Slow-ish request (search with many results)
            curl -s "${BASE_URL}/owners?lastName=" > /dev/null 2>&1
            sleep 0.5

            # Hit a non-existent page (generates 404 errors)
            curl -s "${BASE_URL}/does-not-exist-$(date +%s)" > /dev/null 2>&1
            sleep 0.2

            # POST to create data (if endpoint exists)
            curl -s -X POST "${BASE_URL}/owners/new" \
                -d "firstName=Stress&lastName=Test$(date +%s)&address=123+St&city=Test&telephone=1234567890" \
                > /dev/null 2>&1
            sleep 1
        done
    ) &
    echo $! > "$PID_DIR/slow-requests.pids"

    echo -e "${GREEN}✓ Started mixed-latency request generator${NC}"
    echo -e "  Watch p95/p99: curl ${BASE_URL}/actuator/metrics/http.server.requests"
    echo -e "  Stop with: $0 stop"
    ;;

error-spike)
    echo -e "${CYAN}▸ Error Spike: generating 4xx/5xx responses${NC}"
    echo ""

    (
        while true; do
            # 404s
            curl -s "${BASE_URL}/nonexistent-$(shuf -i 1-1000 -n 1)" > /dev/null 2>&1
            # Invalid owner IDs
            curl -s "${BASE_URL}/owners/99999" > /dev/null 2>&1
            # Bad method
            curl -s -X DELETE "${BASE_URL}/owners/1" > /dev/null 2>&1
            sleep 0.3
        done
    ) &
    echo $! > "$PID_DIR/error-spike.pids"

    echo -e "${GREEN}✓ Started error generator${NC}"
    echo -e "  Watch error rates in HTTP metrics"
    echo -e "  Stop with: $0 stop"
    ;;

mixed)
    echo -e "${CYAN}▸ Mixed Scenario: all stress patterns combined${NC}"
    echo ""
    $0 gc-pressure
    sleep 1
    $0 slow-requests
    sleep 1
    $0 error-spike
    echo ""
    echo -e "${GREEN}✓ All stress patterns running${NC}"
    echo -e "  Stop with: $0 stop"
    ;;

stop)
    echo -e "${CYAN}▸ Stopping all stress tests...${NC}"

    KILLED=0
    for pidfile in "$PID_DIR"/*.pids; do
        [ -f "$pidfile" ] || continue
        while read -r pid; do
            if kill "$pid" 2>/dev/null; then
                ((KILLED++))
            fi
        done < "$pidfile"
        rm -f "$pidfile"
    done

    # Also kill any oha instances
    pkill -f "oha.*${APP_HOST}" 2>/dev/null && ((KILLED++)) || true

    echo -e "${GREEN}✓ Killed $KILLED processes${NC}"
    ;;

status)
    echo -e "${CYAN}▸ Running stress tests:${NC}"
    RUNNING=0
    for pidfile in "$PID_DIR"/*.pids; do
        [ -f "$pidfile" ] || continue
        NAME=$(basename "$pidfile" .pids)
        COUNT=0
        while read -r pid; do
            kill -0 "$pid" 2>/dev/null && ((COUNT++)) || true
        done < "$pidfile"
        if [ "$COUNT" -gt 0 ]; then
            echo -e "  ${GREEN}●${NC} $NAME ($COUNT processes)"
            ((RUNNING+=COUNT))
        else
            echo -e "  ${RED}○${NC} $NAME (stopped)"
            rm -f "$pidfile"
        fi
    done
    [ "$RUNNING" -eq 0 ] && echo -e "  ${YELLOW}No stress tests running${NC}"
    ;;

help|*)
    echo "Usage: $0 <scenario>"
    echo ""
    echo "Scenarios:"
    echo "  gc-pressure    Spike allocation rate → frequent GC pauses"
    echo "  high-load      Saturate Tomcat thread pool → thread contention"
    echo "  slow-requests  Mix of fast/slow/error requests → latency spikes"
    echo "  error-spike    Generate 4xx/5xx errors → error rate anomalies"
    echo "  mixed          All of the above combined"
    echo "  stop           Stop all running stress tests"
    echo "  status         Show running stress tests"
    ;;

esac
