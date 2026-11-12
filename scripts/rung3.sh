#!/usr/bin/env bash
# Rung 3: blargg cpu_instrs 01, 03-11 (02 fails the documented way: no timer) and the
# Mooneye CPU ROMs sm83-python runs, natively in the test-ROM harness.
# Usage: scripts/rung3.sh [REFERENCES_DIR]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
REFS=${1:-"$ROOT/external/references"}
cargo build --release -q --manifest-path "$ROOT/Cargo.toml"
exec nice "$ROOT/target/release/roms" "$REFS/gb-test-roms/cpu_instrs/individual" \
    "$(ls -d "$REFS"/mooneye/mts-*/acceptance)"
