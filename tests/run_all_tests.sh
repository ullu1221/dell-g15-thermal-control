#!/usr/bin/env bash
# ==============================================================================
# Complete Test Runner for Dell G15 Thermal Control Suite
# Validates syntax, unit tests, and live hardware ACPI transitions.
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

echo "=========================================================="
echo " Running Dell G15 Thermal Control Suite Verification"
echo "=========================================================="

# 1. Syntax Verification
echo "[1/3] Verifying Python & Bash syntax..."
python3 -m py_compile \
    "$REPO_DIR"/scripts/dell-g15-thermal \
    "$SCRIPT_DIR"/test_thermal.py

bash -n "$REPO_DIR"/install.sh
echo "  -> Syntax: 100% Valid."

# 2. Native Rust Engine Verification
echo "[2/3] Verifying Native Rust Crate (g15-sentinel)..."
cargo test --manifest-path "$REPO_DIR/native/Cargo.toml"
echo "  -> Rust Unit Tests: 100% Passed."

# 3. Automated Test Suite
echo "[3/3] Launching Thermal & ACPI Test Suite..."
python3 "$SCRIPT_DIR"/test_thermal.py

echo ""
echo "=========================================================="
echo " [VERIFICATION COMPLETE] All thermal tests passed cleanly!"
echo "=========================================================="
