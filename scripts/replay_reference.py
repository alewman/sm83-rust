#!/usr/bin/env python3
"""The reference's half of rung 5: sm83-python's own ``replay_file``, printed as the same
JSON as ``target/release/replay``. Run it with the reference's interpreter.

    replay_reference.py ROM ERROR_LOG [--cycles strict|report|off] [--limit N]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from sm83_python.replay import replay_file


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("rom", type=Path)
    parser.add_argument("log", type=Path)
    parser.add_argument("--cycles", default="strict")
    parser.add_argument("--limit", type=int)
    args = parser.parse_args()
    result = replay_file(args.rom, args.log, limit=args.limit, cycles=args.cycles)
    d = result.divergence
    print(
        json.dumps(
            {
                "compared": result.compared,
                "interrupts": result.interrupts,
                "skipped_cycle_checks": result.skipped_cycle_checks,
                "cycle_mismatches": result.cycle_mismatches,
                "divergence": None
                if d is None
                else {"position": d.position, "pc": d.line.pc, "problems": list(d.problems)},
            },
            separators=(",", ":"),
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
