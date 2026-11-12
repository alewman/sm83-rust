#!/usr/bin/env bash
# Rung 5: MAME replay. Each trace directory (cart.gb + error.log, from sm83-python's
# scripts/mame_trace.py --io) is replayed by the reference's replay_file and by this crate's
# transcription; the two results (lines compared, interrupts raised, HALT groups skipped,
# the divergence's position and problem lines) must be identical. Strict and report modes.
# Usage: scripts/rung5.sh [TRACES_DIR] [REFERENCE_PYTHON]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TRACES=${1:-"$ROOT/external/mame_traces"}
PYTHON=${2:-"$ROOT/external/sm83-python/.venv/bin/python"}
cargo build --release -q --manifest-path "$ROOT/Cargo.toml"
total=0; same=0
for dir in "$TRACES"/*/; do
    for mode in strict report; do
        ref=$(nice "$PYTHON" "$ROOT/scripts/replay_reference.py" "$dir/cart.gb" "$dir/error.log" --cycles $mode)
        ours=$(nice "$ROOT/target/release/replay" "$dir/cart.gb" "$dir/error.log" --cycles $mode)
        total=$((total + 1))
        if [ "$ref" = "$ours" ]; then
            same=$((same + 1))
            echo "$(basename "$dir") ($mode): identical $ours"
        else
            echo "$(basename "$dir") ($mode): DIFFERENT"
            echo "  reference: $ref"
            echo "  rust:      $ours"
        fi
    done
done
echo "$same of $total replays identical"
[ "$same" = "$total" ]
