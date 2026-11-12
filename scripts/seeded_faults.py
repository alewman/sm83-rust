#!/usr/bin/env python3
"""A gate that cannot fail proves nothing: build the crate with one fault at a time and
check that some rung catches each.

Each fault is one exact text replacement in a copy of ``src/`` (with ``tests/`` and
``Cargo.*``), built into a shared fault target directory; then, in order until one fails:
``cargo test`` (what CI runs), the SingleStepTests sample (100 cases a file), the test ROMs
(rung 3), the rung-4 lockstep scenarios, the die all-opcodes ROM in lockstep (rung 1's
cheapest program), and the blargg MAME replays (rung 5). A fault nothing catches is MISSED.

    seeded_faults.py [--refs DIR] [--traces DIR] [--python REFERENCE_PYTHON] [--only NAME]
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / "external" / "faults"

# name: (file, exact text, replacement)
FAULTS = {
    "late-ignored": (
        "core.rs",
        "let queue = enabled & self.if_ & !(late & !before) & INTERRUPT_BITS;",
        "let queue = enabled & self.if_ & INTERRUPT_BITS;",
    ),
    "halt-sees-ignored": (
        "core.rs",
        "            self.pending_at_fetch = true; // only a HALT fetched in this cycle reads it",
        "            let _ = 0;",
    ),
    "jr-drives-pc": (
        "control.rs",
        "self.idle(bus, self.pc & 0xFF00, 0);",
        "self.idle(bus, self.pc, 0);",
    ),
    "halted-drives-pc": (
        "core.rs",
        "self.idle(bus, 0x0000, 0); // a halted CPU drives $0000",
        "self.idle(bus, self.pc, 0); // a halted CPU drives $0000",
    ),
    "halt-one-cycle": (
        "control.rs",
        "        self.read(bus, self.pc, 0);\n        self.halted = true;",
        "        self.halted = true;",
    ),
    "ei-no-delay": (
        "core.rs",
        "let effective_ime = self.ime;",
        "let effective_ime = self.ime || self.ei_pending;",
    ),
    "daa-adjusted-nibble": (
        "alu.rs",
        "if self.f & FLAG_H != 0 || self.a & 0x0F > 0x09 {",
        "if self.f & FLAG_H != 0 || a & 0x0F > 0x09 {",
    ),
    "ie-written-after-callback": (
        "core.rs",
        "        if address == IE_ADDRESS {\n            self.ie = value;\n        }\n        bus.write(address, value, self);",
        "        bus.write(address, value, self);\n        if address == IE_ADDRESS {\n            self.ie = value;\n        }",
    ),
    "ei-halt-pc-not-back": (
        "core.rs",
        "            self.idle(bus, self.pc, -1);\n            self.pc = self.pc.wrapping_sub(1);",
        "            self.idle(bus, self.pc, -1);",
    ),
    "stop-if-zero": (
        "control.rs",
        "self.stop_if = self.if_ & INTERRUPT_BITS;",
        "self.stop_if = 0;",
    ),
    "add-sp-h-off-by-one": (
        "alu.rs",
        "self.f = if (self.sp & 0xF) + (offset & 0xF) as u16 > 0xF {",
        "self.f = if (self.sp & 0xF) + (offset & 0xF) as u16 >= 0xF {",
    ),
    "dispatch-m2-address": (
        "core.rs",
        "self.idle(bus, self.pc.wrapping_add(1), -1); // M2",
        "self.idle(bus, self.pc, -1); // M2",
    ),
}


def ok(command: list[str], **kwargs) -> bool:
    return subprocess.run(["nice", *command], capture_output=True, text=True, **kwargs).returncode == 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--refs", type=Path, default=ROOT / "external" / "references")
    parser.add_argument("--traces", type=Path, default=ROOT / "external" / "mame_traces")
    parser.add_argument("--python", default=str(ROOT / "external" / "sm83-python" / ".venv" / "bin" / "python"))
    parser.add_argument("--only")
    args = parser.parse_args()
    target = WORK / "target"
    blargg = args.refs / "gb-test-roms" / "cpu_instrs" / "individual"
    mooneye = next((args.refs / "mooneye").glob("mts-*/acceptance"))
    scenarios = sorted((ROOT / "conformance" / "scenarios").glob("*.json"))
    scenarios += sorted((ROOT / "external" / "scenarios").glob("die-halt-*.json"))
    scenarios += sorted((ROOT / "external" / "scenarios").glob("die-timer-*.json"))
    die = ROOT / "external" / "scenarios" / "die-all-opcodes.json"
    replays = []
    for directory in sorted(p for p in args.traces.glob("blargg-*") if (p / "error.log").exists()):
        rom, log = directory / "cart.gb", directory / "error.log"
        reference = subprocess.run(
            [args.python, str(ROOT / "scripts" / "replay_reference.py"), str(rom), str(log)],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        replays.append((rom, log, reference))
    caught = missed = 0
    for name, (file, old, new) in FAULTS.items():
        if args.only and name != args.only:
            continue
        tree = WORK / name
        if tree.exists():
            shutil.rmtree(tree)
        tree.mkdir(parents=True)
        for part in ("src", "tests"):
            shutil.copytree(ROOT / part, tree / part)
        for part in ("Cargo.toml", "Cargo.lock"):
            shutil.copy(ROOT / part, tree / part)
        source = tree / "src" / file
        text = source.read_text()
        if text.count(old) != 1:
            print(f"{name}: the fault's text is not in {file} exactly once")
            return 2
        source.write_text(text.replace(old, new))
        env_target = ["--target-dir", str(target)]
        if not ok(["cargo", "build", "--release", "-q", "--manifest-path", str(tree / "Cargo.toml"), *env_target]):
            print(f"{name}: does not build")
            return 2
        bin_dir = target / "release"
        checks = [
            ("cargo test", lambda: ok(["cargo", "test", "--release", "-q", "--manifest-path", str(tree / "Cargo.toml"), *env_target])),
            ("rung 2 sample", lambda: ok([str(bin_dir / "sst"), str(args.refs / "sm83" / "v1"), "--limit", "100"])),
            ("rung 3", lambda: ok([str(bin_dir / "roms"), str(blargg), str(mooneye)])),
            ("rung 4", lambda: ok(["python3", str(ROOT / "scripts" / "lockstep.py"), *map(str, scenarios), "--python", args.python, "--rust", str(bin_dir / "trace"), "--traces", str(WORK / "traces"), "--jobs", "10"])),
            ("rung 1 (die ROM)", lambda: ok(["python3", str(ROOT / "scripts" / "lockstep.py"), str(die), "--python", args.python, "--rust", str(bin_dir / "trace"), "--traces", str(WORK / "traces")])),
            ("rung 5", lambda: all(
                subprocess.run([str(bin_dir / "replay"), str(rom), str(log)], capture_output=True, text=True).stdout.strip() == ref
                for rom, log, ref in replays
            )),
        ]  # fmt: skip
        by = [label for label, check in checks if not check()]
        if by:
            caught += 1
            print(f"{name}: caught by {', '.join(by)}", flush=True)
        else:
            missed += 1
            print(f"{name}: MISSED", flush=True)
        shutil.rmtree(tree)
    print(f"{caught} caught, {missed} missed")
    return 1 if missed else 0


if __name__ == "__main__":
    sys.exit(main())
