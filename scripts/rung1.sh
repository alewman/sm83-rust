#!/usr/bin/env bash
# Rung 1: trace lockstep with the reference on programs: blargg cpu_instrs 01-11 and the
# Mooneye CPU ROMs in the test-ROM harness (first STEPS boundaries each, default 200,000),
# and the die all-opcodes ROM on the mock SoC; every M-cycle compared.
# Usage: scripts/rung1.sh [REFERENCES_DIR] [REFERENCE_PYTHON]
#   REFERENCES_DIR holds gb-test-roms/ and mooneye/ (scripts/fetch_references.sh).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
REFS=${1:-"$ROOT/external/references"}
PYTHON=${2:-"$ROOT/external/sm83-python/.venv/bin/python"}
"$PYTHON" "$ROOT/scripts/make_scenarios.py" --steps "${STEPS:-200000}" \
    --blargg "$REFS/gb-test-roms/cpu_instrs/individual" \
    --mooneye "$(ls -d "$REFS"/mooneye/mts-*/acceptance)" > /dev/null
exec python3 "$ROOT/scripts/lockstep.py" "$ROOT"/external/scenarios/blargg-*.json \
    "$ROOT"/external/scenarios/mooneye-*.json "$ROOT"/external/scenarios/die-all-opcodes.json \
    --python "$PYTHON" --jobs "${JOBS:-8}"
