#!/usr/bin/env bash
##############################################################################
# jvmtui — Grab GC Log from Docker container
#
# Copies the GC log from the running Petclinic container into fixtures/
# for use as test data during parser development.
#
# Usage:
#   ./scripts/grab-gclog.sh
#   ./scripts/grab-gclog.sh --lines 500        # grab last 500 lines
#   ./scripts/grab-gclog.sh --container myapp   # different container name
##############################################################################

set -euo pipefail

CONTAINER="${CONTAINER:-jvmtui-target}"
GC_LOG_PATH="/app/gc.log"
FIXTURE_DIR="./fixtures"
LINES=200

while [[ $# -gt 0 ]]; do
    case $1 in
        --lines)     LINES="$2"; shift 2 ;;
        --container) CONTAINER="$2"; shift 2 ;;
        --path)      GC_LOG_PATH="$2"; shift 2 ;;
        *) shift ;;
    esac
done

mkdir -p "$FIXTURE_DIR"

echo "Grabbing last $LINES lines of GC log from $CONTAINER:$GC_LOG_PATH..."

if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER}$"; then
    echo "ERROR: Container '$CONTAINER' is not running."
    echo "Start it with: cd docker && docker compose up -d"
    exit 1
fi

# Grab the full log for analysis
docker exec "$CONTAINER" cat "$GC_LOG_PATH" > "${FIXTURE_DIR}/gc_full.log" 2>/dev/null

TOTAL_LINES=$(wc -l < "${FIXTURE_DIR}/gc_full.log")
echo "Full GC log: $TOTAL_LINES lines"

# Save the tail as the primary fixture
tail -"$LINES" "${FIXTURE_DIR}/gc_full.log" > "${FIXTURE_DIR}/gc_sample.log"
echo "Saved: fixtures/gc_sample.log (last $LINES lines)"

# Detect collector type
COLLECTOR="unknown"
if grep -q "G1 Evacuation Pause\|Pause Young (Normal)\|Pause Young (Concurrent Start)" "${FIXTURE_DIR}/gc_sample.log" 2>/dev/null; then
    COLLECTOR="G1GC"
elif grep -q "ZGC\|Z:" "${FIXTURE_DIR}/gc_sample.log" 2>/dev/null; then
    COLLECTOR="ZGC"
elif grep -q "Shenandoah\|Pause Init Mark" "${FIXTURE_DIR}/gc_sample.log" 2>/dev/null; then
    COLLECTOR="Shenandoah"
fi

echo "Detected collector: $COLLECTOR"

# Extract just the pause events for a cleaner fixture
grep -E "Pause (Young|Full|Mixed|Init Mark|Final Mark|Init Update|Final Update)" "${FIXTURE_DIR}/gc_full.log" \
    > "${FIXTURE_DIR}/gc_pauses_only.log" 2>/dev/null || true

PAUSE_COUNT=$(wc -l < "${FIXTURE_DIR}/gc_pauses_only.log")
echo "Extracted $PAUSE_COUNT pause events → fixtures/gc_pauses_only.log"

# Show last 5 events
echo ""
echo "── Last 5 GC events ──"
tail -5 "${FIXTURE_DIR}/gc_pauses_only.log"

echo ""
echo "Done. Fixtures ready for parser development."
