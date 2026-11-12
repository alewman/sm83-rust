#!/usr/bin/env python3
"""Rungs 1 and 4: run each scenario on both cores and compare the traces.

    lockstep.py SCENARIO.json [...] [--jobs N] [--python REFERENCE_PYTHON]

Each scenario runs on sm83-python (``scripts/reference_trace.py`` on the
reference's interpreter) and on this crate (``target/release/trace``), into
``external/traces/``; the two traces are compared with the reference's own
``first_trace_divergence`` (``scripts/compare_traces.py``). Stdlib only.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# A small PyPy nursery: several reference processes at once otherwise share one L3.
ENV = {**os.environ, "PYPY_GC_NURSERY": os.environ.get("PYPY_GC_NURSERY", "1M")}
TRACES = ROOT / "external" / "traces"


def one(scenario: Path, python: str, rust_binary: Path, traces: Path) -> tuple[str, bool, str]:
    name = scenario.stem
    ref, ours = traces / f"{name}.ref.jsonl", traces / f"{name}.rust.jsonl"
    nice = ["nice"]
    reference = subprocess.run(
        [*nice, python, str(ROOT / "scripts" / "reference_trace.py"), str(scenario), "--out", str(ref)],
        capture_output=True, text=True, env=ENV,
    )
    if reference.returncode:
        return name, False, "reference failed: " + reference.stderr.strip()[-400:]
    rust = subprocess.run(
        [*nice, str(rust_binary), str(scenario), "--out", str(ours)],
        capture_output=True, text=True,
    )
    if rust.returncode:
        return name, False, "rust failed: " + rust.stderr.strip()[-400:]
    compared = subprocess.run(
        [python, str(ROOT / "scripts" / "compare_traces.py"), str(ref), str(ours)],
        capture_output=True, text=True, env=ENV,
    )
    lines = sum(1 for _ in ref.open())
    text = compared.stdout.strip()
    if compared.returncode == 0:
        ref.unlink()
        ours.unlink()
        return name, True, f"{text} ({lines:,} boundaries)"
    return name, False, text[:2000]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("scenarios", nargs="+", type=Path)
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--python", default=str(ROOT / "external" / "sm83-python" / ".venv" / "bin" / "python"))
    parser.add_argument("--rust", type=Path, help="a trace binary to judge instead of this tree's")
    parser.add_argument("--traces", type=Path, default=TRACES)
    args = parser.parse_args()
    args.traces.mkdir(parents=True, exist_ok=True)
    rust_binary = args.rust or ROOT / "target" / "release" / "trace"
    if args.rust is None:
        build = subprocess.run(["cargo", "build", "--release", "-q", "--manifest-path", str(ROOT / "Cargo.toml")])
        if build.returncode:
            return 1
    failed = 0
    with ThreadPoolExecutor(args.jobs) as pool:
        for name, ok, text in pool.map(lambda s: one(s, args.python, rust_binary, args.traces), args.scenarios):
            print(f"{name}: {text}", flush=True)
            failed += not ok
    print(f"{len(args.scenarios) - failed} of {len(args.scenarios)} scenarios identical")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
