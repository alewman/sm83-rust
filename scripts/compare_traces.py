#!/usr/bin/env python3
"""Compare two boundary traces with the reference's own reader and comparison.

    compare_traces.py REFERENCE.jsonl OURS.jsonl

Prints "traces are identical" and exits 0, or the first divergence (its
position and every differing field) and exits 1. Run it with the reference's
interpreter.
"""

from __future__ import annotations

import json
import sys

from sm83_python.trace import first_trace_divergence, read_trace


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    with open(sys.argv[1]) as left, open(sys.argv[2]) as right:
        divergence = first_trace_divergence(read_trace(left), read_trace(right))
    if divergence is None:
        print("traces are identical")
        return 0
    print(json.dumps(divergence.as_dict(), indent=1))
    return 1


if __name__ == "__main__":
    sys.exit(main())
