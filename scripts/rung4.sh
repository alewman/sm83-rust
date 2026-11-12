#!/usr/bin/env bash
# Rung 4: interrupts, HALT and STOP in lockstep with the reference: the scenarios of
# test_special_states.py, every opcode from random states (test_bus_schedule.py widened),
# the interrupt workout under seeded random requests (halt_sees and late included), and the
# die-run HALT programs on the mock SoC.
# Usage: scripts/rung4.sh [REFERENCE_PYTHON]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
PYTHON=${1:-"$ROOT/external/sm83-python/.venv/bin/python"}
"$PYTHON" "$ROOT/scripts/make_scenarios.py" > /dev/null
exec python3 "$ROOT/scripts/lockstep.py" "$ROOT"/conformance/scenarios/*.json \
    "$ROOT"/external/scenarios/die-halt-*.json "$ROOT"/external/scenarios/die-timer-*.json \
    --python "$PYTHON" --jobs "${JOBS:-8}"
