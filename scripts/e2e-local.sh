#!/usr/bin/env bash
##############################################################################
# e2e-local.sh — end-to-end smoke test against a real JVM, no Docker needed.
#
# Requires: java 17+, curl, jq. Downloads the Jolokia agent and generates a
# Spring Boot app via start.spring.io, runs it with G1 GC logging, then
# asserts that `drishti-jvm --once --json` sees real data from both sources.
#
# Usage: ./scripts/e2e-local.sh
##############################################################################
set -euo pipefail

WORK="${E2E_DIR:-/tmp/drishti-e2e}"
JOLOKIA_VERSION=2.1.1
mkdir -p "$WORK"

cleanup() {
    [ -f "$WORK/app.pid" ] && kill "$(cat "$WORK/app.pid")" 2>/dev/null || true
}
trap cleanup EXIT

echo "==> Building drishti-jvm"
cargo build -p drishti-jvm

echo "==> Fetching Jolokia agent $JOLOKIA_VERSION"
[ -f "$WORK/jolokia-agent.jar" ] || curl -sL -o "$WORK/jolokia-agent.jar" \
    "https://repo1.maven.org/maven2/org/jolokia/jolokia-agent-jvm/$JOLOKIA_VERSION/jolokia-agent-jvm-$JOLOKIA_VERSION-javaagent.jar"

if ! ls "$WORK/target-app/target/"*.jar >/dev/null 2>&1; then
    echo "==> Generating Spring Boot target app (start.spring.io)"
    curl -sL -o "$WORK/target.zip" \
        "https://start.spring.io/starter.zip?type=maven-project&language=java&javaVersion=21&dependencies=web,actuator,prometheus&name=drishti-target&artifactId=drishti-target"
    unzip -q -o "$WORK/target.zip" -d "$WORK/target-app"
    cat >> "$WORK/target-app/src/main/resources/application.properties" << 'EOF'
management.endpoints.web.exposure.include=health,info,metrics,prometheus,threaddump,loggers,logfile
management.endpoint.health.show-details=always
logging.file.name=app.log
EOF
    echo "==> Building target app (first run downloads Maven deps)"
    (cd "$WORK/target-app" && ./mvnw -q package -DskipTests)
fi

echo "==> Starting target JVM (G1, 256M heap, Jolokia agent, GC log)"
java -Xms256m -Xmx256m -XX:+UseG1GC \
    "-Xlog:gc*:file=$WORK/gc.log:time,uptime,level,tags" \
    "-javaagent:$WORK/jolokia-agent.jar=port=8778,host=127.0.0.1" \
    -jar "$WORK/target-app/target/"*.jar > "$WORK/boot.log" 2>&1 &
echo $! > "$WORK/app.pid"

echo "==> Waiting for actuator health"
for _ in $(seq 1 60); do
    curl -sf http://localhost:8080/actuator/health >/dev/null 2>&1 && break
    sleep 1
done
curl -sf http://localhost:8080/actuator/health >/dev/null

# Generate a little traffic
for _ in $(seq 1 5); do curl -s http://localhost:8080/actuator/health >/dev/null; done

echo "==> Asserting --once --json sees real data from both sources"
SNAP=$(./target/debug/drishti-jvm --once --json 2>/dev/null)

check() {
    local desc=$1 expr=$2
    if echo "$SNAP" | jq -e "$expr" >/dev/null; then
        echo "  PASS: $desc"
    else
        echo "  FAIL: $desc ($expr)"
        exit 1
    fi
}

check "heap.used > 0 (actuator)"          '.heap.used > 0'
check "threads live > 0 (actuator)"       '.thread_summary.live > 0'
check "vm name set (jolokia)"             '.jvm_info.vm_name | length > 0'
check "uptime > 0 (jolokia)"              '.jvm_info.uptime_ms > 0'
check "G1 detected (jolokia)"             '.jvm_info.gc_algorithm == "G1"'
check "gc collectors present (jolokia)"   '.gc_collectors | length > 0'
check "memory pools present"              '.memory_pools | length > 0'
check "classes loaded > 0"                '.classes.loaded > 0'

echo "==> Recommendations mode"
./target/debug/drishti-jvm --once --recommendations 2>/dev/null

echo ""
echo "e2e OK — all assertions passed against a live JVM"
