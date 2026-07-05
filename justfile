# ============================================================
# jvmtui — Task Runner (requires 'just': cargo install just)
# ============================================================
# Usage:
#   just              # list all tasks
#   just setup        # Phase 0: setup environment
#   just lab-up       # start lab Spring Boot app
#   just verify       # verify all data sources
#   just explore heap # explore API interactively
# ============================================================

# Default: list tasks
default:
    @just --list

# ── Phase 0: Environment ──────────────────────────────────────

# Setup Rust toolchain and dev tools
setup:
    ./scripts/setup-env.sh

# Start the lab Spring Boot app (Petclinic + Jolokia)
lab-up:
    cd docker && docker compose up -d
    @echo ""
    @echo "Waiting for Petclinic to start..."
    @timeout 120 bash -c 'until curl -sf http://localhost:8080/actuator/health > /dev/null 2>&1; do sleep 2; echo -n "."; done'
    @echo ""
    @echo "✓ Petclinic ready at http://localhost:8080"
    @echo "✓ Jolokia ready at http://localhost:8778"

# Stop the lab app
lab-down:
    cd docker && docker compose down

# Show lab app logs
lab-logs:
    cd docker && docker compose logs -f petclinic

# Verify all data sources and generate fixtures
verify:
    ./scripts/verify-sources.sh

# Grab GC log from container into fixtures/
grab-gclog:
    ./scripts/grab-gclog.sh

# ── API Exploration ───────────────────────────────────────────

# Explore API sections: heap, gc, threads, cpu, http, hikari, health, loggers, bulk, all
explore section="help":
    ./scripts/api-cheatsheet.sh {{section}}

# ── Stress Testing ────────────────────────────────────────────

# Run stress test: gc-pressure, high-load, slow-requests, error-spike, mixed
stress scenario="help":
    ./scripts/stress-test.sh {{scenario}}

# Stop all stress tests
stress-stop:
    ./scripts/stress-test.sh stop

# ── Phase 0 Complete Workflow ─────────────────────────────────

# Run the full Phase 0 workflow
phase0: setup lab-up verify grab-gclog
    @echo ""
    @echo "═══════════════════════════════════════════════"
    @echo "  Phase 0 Complete!"
    @echo "  Fixtures saved in ./fixtures/"
    @echo "  Ready for Phase 1: cargo workspace scaffold"
    @echo "═══════════════════════════════════════════════"

# ── Phase 1+ (placeholders) ──────────────────────────────────

# Build the full workspace
build:
    cargo build --workspace

# Run all tests
test:
    cargo nextest run --workspace

# Run clippy
lint:
    cargo clippy --workspace -- -D warnings

# Run the TUI
run *ARGS:
    cargo run -p drishti-tui -- {{ARGS}}

# Watch mode development
dev:
    bacon clippy
