#!/usr/bin/env bash
##############################################################################
# jvmtui — Phase 0 Environment Setup
#
# Sets up Rust toolchain, development tools, and verifies everything works.
#
# Usage:
#   ./scripts/setup-env.sh
#   ./scripts/setup-env.sh --skip-rust   # if Rust is already installed
##############################################################################

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

SKIP_RUST=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-rust) SKIP_RUST=true; shift ;;
        *) shift ;;
    esac
done

step() { echo -e "\n${BOLD}${CYAN}▸ $1${NC}"; }
ok()   { echo -e "  ${GREEN}✓${NC} $1"; }
warn() { echo -e "  ${YELLOW}⚠${NC} $1"; }
fail() { echo -e "  ${RED}✗${NC} $1"; }

echo -e "${BOLD}╔══════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}║      jvmtui — Development Environment Setup     ║${NC}"
echo -e "${BOLD}╚══════════════════════════════════════════════════╝${NC}"

# ── 1. Rust toolchain ─────────────────────────────────────────────────────

step "1. Rust Toolchain"

if [ "$SKIP_RUST" = false ]; then
    if command -v rustup &> /dev/null; then
        ok "rustup already installed"
        rustup update stable 2>&1 | tail -1
    else
        echo "  Installing Rust via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi
else
    ok "Skipping Rust install (--skip-rust)"
fi

# Verify
if command -v rustc &> /dev/null; then
    RUST_VER=$(rustc --version)
    ok "rustc: $RUST_VER"
else
    fail "rustc not found. Run without --skip-rust"
    exit 1
fi

if command -v cargo &> /dev/null; then
    CARGO_VER=$(cargo --version)
    ok "cargo: $CARGO_VER"
else
    fail "cargo not found"
    exit 1
fi

# ── 2. Rust components ────────────────────────────────────────────────────

step "2. Rust Components"

for component in clippy rustfmt; do
    if rustup component list --installed | grep -q "$component"; then
        ok "$component already installed"
    else
        rustup component add "$component"
        ok "$component installed"
    fi
done

# rust-analyzer (check if available)
if command -v rust-analyzer &> /dev/null || rustup component list --installed | grep -q "rust-analyzer"; then
    ok "rust-analyzer available"
else
    rustup component add rust-analyzer 2>/dev/null && ok "rust-analyzer installed" || warn "rust-analyzer not available via rustup (install separately for your editor)"
fi

# ── 3. Cargo development tools ────────────────────────────────────────────

step "3. Cargo Development Tools"

CARGO_TOOLS=(
    "cargo-watch"
    "cargo-nextest"
    "cargo-deny"
    "bacon"
)

for tool in "${CARGO_TOOLS[@]}"; do
    BIN_NAME=$(echo "$tool" | sed 's/cargo-//')
    if cargo install --list 2>/dev/null | grep -q "^${tool} "; then
        ok "$tool already installed"
    else
        echo "  Installing $tool..."
        cargo install "$tool" --quiet 2>&1 && ok "$tool installed" || warn "$tool install failed (non-critical)"
    fi
done

# ── 4. System dependencies ────────────────────────────────────────────────

step "4. System Dependencies"

# Check for essentials
for cmd in curl jq git docker; do
    if command -v "$cmd" &> /dev/null; then
        ok "$cmd: $(command -v $cmd)"
    else
        warn "$cmd not found — install it for full workflow"
    fi
done

# Check for oha (load generator, optional)
if command -v oha &> /dev/null; then
    ok "oha (load generator) available"
else
    echo "  Installing oha (HTTP load generator)..."
    cargo install oha --quiet 2>&1 && ok "oha installed" || warn "oha install failed (non-critical, can use curl instead)"
fi

# ── 5. Docker check ───────────────────────────────────────────────────────

step "5. Docker Environment"

if command -v docker &> /dev/null; then
    if docker info &> /dev/null; then
        ok "Docker daemon running"
        COMPOSE_CMD=""
        if docker compose version &> /dev/null; then
            COMPOSE_CMD="docker compose"
            ok "Docker Compose v2 available"
        elif command -v docker-compose &> /dev/null; then
            COMPOSE_CMD="docker-compose"
            ok "Docker Compose v1 available"
        else
            warn "Docker Compose not found — needed for the lab environment"
        fi
    else
        warn "Docker installed but daemon not running"
    fi
else
    warn "Docker not installed — needed for the lab Spring Boot environment"
    echo -e "    ${CYAN}Install: https://docs.docker.com/engine/install/${NC}"
fi

# ── 6. Compile test ───────────────────────────────────────────────────────

step "6. Quick Compile Test (verifying crate availability)"

TEMP_DIR=$(mktemp -d)
cd "$TEMP_DIR"

cat > Cargo.toml << 'EOF'
[package]
name = "jvmtui-test"
version = "0.1.0"
edition = "2021"

[dependencies]
ratatui = "0.30"
crossterm = { version = "0.29", features = ["event-stream"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "sync"] }
reqwest = { version = "0.13", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
EOF

cat > src/main.rs << 'RUST'
use ratatui::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct HealthCheck {
    status: String,
}

#[tokio::main]
async fn main() {
    println!("All core dependencies compile successfully!");
}
RUST

mkdir -p src

echo "  Fetching and compiling core dependencies (this may take a few minutes first time)..."
if cargo check --quiet 2>&1; then
    ok "All core crates (ratatui, tokio, reqwest, serde) compile cleanly"
else
    fail "Compilation failed — check Rust version and network connectivity"
fi

cd - > /dev/null
rm -rf "$TEMP_DIR"

# ── Summary ────────────────────────────────────────────────────────────────

echo ""
echo -e "${BOLD}╔══════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}║              Setup Complete                      ║${NC}"
echo -e "${BOLD}╚══════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${BOLD}Next steps:${NC}"
echo ""
echo -e "  ${CYAN}1. Start the lab environment:${NC}"
echo -e "     cd docker && docker compose up -d"
echo -e "     docker compose logs -f petclinic  # wait for 'Started PetClinicApplication'"
echo ""
echo -e "  ${CYAN}2. Verify all data sources:${NC}"
echo -e "     ./scripts/verify-sources.sh"
echo ""
echo -e "  ${CYAN}3. If using Docker, grab the GC log fixture:${NC}"
echo -e "     docker compose exec petclinic cat /app/gc.log | tail -200 > fixtures/gc_sample.log"
echo ""
echo -e "  ${CYAN}4. Once verify passes, move to Phase 1:${NC}"
echo -e "     Scaffold the Rust workspace"
echo ""
