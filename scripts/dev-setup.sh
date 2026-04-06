#!/usr/bin/env bash
set -euo pipefail

# Usage: dev-setup.sh [--check] [--ci]
#   --check   Non-interactive: report missing prerequisites and exit (exit 1 if any missing)
#   --ci      Auto-accept all install prompts (for CI environments)

# Colors (disabled when not a terminal or --ci flag).
if [[ -t 1 ]] && [[ "${1:-}" != "--ci" ]]; then
    GREEN='\033[0;32m'; YELLOW='\033[0;33m'; RED='\033[0;31m'; RESET='\033[0m'
else
    GREEN=''; YELLOW=''; RED=''; RESET=''
fi

CHECK_ONLY=false
CI_MODE=false
for arg in "$@"; do
    case "$arg" in
        --check) CHECK_ONLY=true ;;
        --ci)    CI_MODE=true ;;
    esac
done

OS="$(uname -s)"
ok()   { printf "${GREEN}  ✓ %s${RESET}\n" "$1"; }
warn() { printf "${YELLOW}  ⚠ %s${RESET}\n" "$1"; }
fail() { printf "${RED}  ✗ %s${RESET}\n" "$1"; }

ask_install() {
    if $CHECK_ONLY; then return 1; fi
    if $CI_MODE; then return 0; fi
    printf "    Install %s? [Y/n] " "$1"
    read -r reply
    [[ -z "$reply" || "$reply" =~ ^[Yy] ]]
}

MISSING=0

echo "Checking Matcher development prerequisites..."
echo ""

# 1. Rust nightly
echo "Rust toolchain:"
if command -v rustup &>/dev/null; then
    ACTIVE=$(rustup show active-toolchain 2>/dev/null || echo "")
    if echo "$ACTIVE" | grep -q nightly; then
        ok "Rust nightly ($ACTIVE)"
    else
        warn "Rust installed but active toolchain is not nightly: $ACTIVE"
        if ask_install "nightly toolchain"; then
            rustup toolchain install nightly
            rustup default nightly
        else
            ((MISSING++))
        fi
    fi
else
    fail "rustup not found"
    echo "    Install: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y"
    ((MISSING++))
fi

# 2. just
echo "Task runner:"
if command -v just &>/dev/null; then
    ok "just $(just --version 2>/dev/null | head -1)"
else
    fail "just not found"
    if ask_install "just (via cargo)"; then
        cargo install just
    else
        echo "    Install: cargo install just"
        ((MISSING++))
    fi
fi

# 3. cargo-nextest
echo "Test runner:"
if cargo nextest --version &>/dev/null; then
    ok "cargo-nextest $(cargo nextest --version 2>/dev/null | head -1)"
else
    fail "cargo-nextest not found"
    if ask_install "cargo-nextest"; then
        cargo install cargo-nextest
    else
        echo "    Install: cargo install cargo-nextest"
        ((MISSING++))
    fi
fi

# 4. cargo-all-features
echo "Feature matrix:"
if cargo check-all-features --version &>/dev/null; then
    ok "cargo-all-features"
else
    fail "cargo-all-features not found"
    if ask_install "cargo-all-features"; then
        cargo install cargo-all-features
    else
        echo "    Install: cargo install cargo-all-features"
        ((MISSING++))
    fi
fi

# 5. cargo-tarpaulin (Linux only)
echo "Coverage:"
if [[ "$OS" == "Linux" ]]; then
    if cargo tarpaulin --version &>/dev/null; then
        ok "cargo-tarpaulin"
    else
        fail "cargo-tarpaulin not found"
        if ask_install "cargo-tarpaulin"; then
            cargo install cargo-tarpaulin
        else
            echo "    Install: cargo install cargo-tarpaulin"
            ((MISSING++))
        fi
    fi
else
    warn "cargo-tarpaulin skipped (Linux only, you're on $OS)"
fi

# 6. uv
echo "Python env:"
if command -v uv &>/dev/null; then
    ok "uv $(uv --version 2>/dev/null | head -1)"
else
    fail "uv not found"
    echo "    Install: curl -LsSf https://astral.sh/uv/install.sh | sh"
    ((MISSING++))
fi

# 7. Python >= 3.8
echo "Python:"
if command -v python3 &>/dev/null; then
    PY_VERSION=$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')
    PY_MAJOR=$(echo "$PY_VERSION" | cut -d. -f1)
    PY_MINOR=$(echo "$PY_VERSION" | cut -d. -f2)
    if [[ "$PY_MAJOR" -ge 3 ]] && [[ "$PY_MINOR" -ge 8 ]]; then
        ok "Python $PY_VERSION"
    else
        fail "Python $PY_VERSION (need >= 3.8)"
        ((MISSING++))
    fi
else
    fail "python3 not found"
    echo "    Install via your system package manager or https://python.org"
    ((MISSING++))
fi

# 8. Java 21+
echo "Java:"
if command -v java &>/dev/null; then
    JAVA_VERSION=$(java -version 2>&1 | head -1 | sed 's/.*"\([0-9]*\).*/\1/')
    if [[ "$JAVA_VERSION" -ge 21 ]]; then
        ok "Java $JAVA_VERSION"
    else
        fail "Java $JAVA_VERSION (need >= 21)"
        ((MISSING++))
    fi
else
    fail "java not found"
    echo "    Install: https://adoptium.net (Temurin JDK 21+)"
    ((MISSING++))
fi

# 9. Maven
echo "Maven:"
if command -v mvn &>/dev/null; then
    ok "Maven $(mvn --version 2>/dev/null | head -1)"
else
    fail "mvn not found"
    echo "    Install via your system package manager or https://maven.apache.org"
    ((MISSING++))
fi

# 10. C compiler
echo "C compiler:"
if command -v cc &>/dev/null; then
    ok "cc ($(cc --version 2>/dev/null | head -1))"
else
    fail "cc not found"
    if [[ "$OS" == "Darwin" ]]; then
        echo "    Install: xcode-select --install"
    else
        echo "    Install: sudo apt install build-essential  (or equivalent)"
    fi
    ((MISSING++))
fi

echo ""
if [[ "$MISSING" -eq 0 ]]; then
    printf "${GREEN}All prerequisites satisfied.${RESET}\n"
    echo "Run 'just build' to build the project."
else
    printf "${YELLOW}%d prerequisite(s) missing.${RESET}\n" "$MISSING"
    echo "Fix the items above, then re-run this script."
fi
