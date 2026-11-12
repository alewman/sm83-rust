#!/usr/bin/env bash
# The third-party test material, at the reference's pins, into external/references/
# (nothing here is committed): SingleStepTests sm83 (rung 2), blargg's test ROMs and the
# Mooneye Test Suite build (rungs 1 and 3). Files are checked against
# scripts/references.sha256, as sm83-python's fetch_references.py checks its pins.
# Usage: scripts/fetch_references.sh [DEST]   (SKIP_SST=1 leaves out the 107 MB corpus)
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEST=${1:-"$ROOT/external/references"}
mkdir -p "$DEST"

clone_at() {  # URL REV DIR: a shallow checkout of one revision
    if [ ! -d "$3/.git" ]; then
        git init -q "$3"
        git -C "$3" remote add origin "$1"
    fi
    git -C "$3" fetch -q --depth 1 origin "$2"
    git -C "$3" checkout -q FETCH_HEAD
}

if [ -z "${SKIP_SST:-}" ]; then
    clone_at https://github.com/SingleStepTests/sm83 f9c30210245dd691661db39f5ace022c465ecc2f "$DEST/sm83"
fi
clone_at https://github.com/retrio/gb-test-roms c240dd7d700e5c0b00a7bbba52b53e4ee67b5f15 "$DEST/gb-test-roms"
ZIP=mts-20260714-0944-31510e1
if [ ! -f "$DEST/mooneye/$ZIP.zip" ]; then
    mkdir -p "$DEST/mooneye"
    curl -sSfL -A "sm83-rust reference fetch" -o "$DEST/mooneye/$ZIP.zip" \
        "https://gekkio.fi/files/mooneye-test-suite/$ZIP/$ZIP.zip"
fi
(cd "$DEST" && sha256sum --quiet -c "$ROOT/scripts/references.sha256")
if [ ! -d "$DEST/mooneye/$ZIP" ]; then
    (cd "$DEST/mooneye" && unzip -q "$ZIP.zip")
fi
echo "references in $DEST"
