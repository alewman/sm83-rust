#!/usr/bin/env bash
# Rung 2: SingleStepTests sm83, 498 files x 1,000 cases (LIMIT=N for the first N cases of
# each file): registers, ime/ei, RAM, and every cycle's bus activity.
# Usage: scripts/rung2.sh [CORPUS_DIR]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CORPUS=${1:-"$ROOT/external/references/sm83/v1"}
cargo build --release -q --manifest-path "$ROOT/Cargo.toml"
exec nice "$ROOT/target/release/sst" "$CORPUS" ${LIMIT:+--limit "$LIMIT"} --jobs "${JOBS:-8}"
