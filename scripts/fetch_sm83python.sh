#!/usr/bin/env bash
# The reference, sm83-python at the pinned commit, into external/ with a venv.
# Usage: scripts/fetch_sm83python.sh [DEST] [SOURCE]
#   SOURCE: a git URL or a local clone (default: GitHub; the repository is private, so
#   GitHub needs credentials). PYTHON picks the interpreter for the venv (PyPy runs the
#   long rungs several times faster).
set -euo pipefail
COMMIT=95f2b8b39c1a1b3890d3b47b39cb1f698e948eca
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEST=${1:-"$ROOT/external/sm83-python"}
SOURCE=${2:-https://github.com/alewman/sm83-python}
PYTHON=${PYTHON:-python3}
if [ ! -d "$DEST/.git" ]; then
    git clone -q "$SOURCE" "$DEST"
fi
git -C "$DEST" fetch -q origin "$COMMIT" 2>/dev/null || true
git -C "$DEST" checkout -q "$COMMIT"
if [ ! -x "$DEST/.venv/bin/python" ]; then
    "$PYTHON" -m venv "$DEST/.venv"
fi
"$DEST/.venv/bin/python" -m pip install -q -e "$DEST"
echo "sm83-python $(git -C "$DEST" rev-parse --short HEAD) in $DEST"
