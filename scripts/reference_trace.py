#!/usr/bin/env python3
"""Run a scenario on sm83-python and write its boundary trace (the reference's half).

The same scenario format as ``src/bin/trace.rs`` (conformance/README.md), run on the
reference core through its own ``DebugSession`` with access tracking, written with its own
``write_trace``. The ``harness`` host is the reference's own ``Harness``; the ``mocksoc``
host is the reference's own ``scripts/die_compare.py`` ``MockSoc``. Run it with the
reference's interpreter (``external/sm83-python/.venv/bin/python``).

    reference_trace.py SCENARIO.json [--out TRACE.jsonl]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import sm83_python
from sm83_python import SM83, CPUState, StopAction, StopResult
from sm83_python.debug import DebugSession
from sm83_python.harness import Harness
from sm83_python.trace import write_trace

REFERENCE = Path(sm83_python.__file__).resolve().parents[2]
sys.path.insert(0, str(REFERENCE / "scripts"))

from die_compare import MockSoc  # noqa: E402

STATE_FIELDS = set(CPUState.__dataclass_fields__)


def act(cpu: SM83, event: dict) -> None:
    action = event["action"]
    if action == "irq":
        cpu.request_interrupt(
            event["bit"], halt_sees=event.get("halt_sees", False), late=event.get("late", False)
        )
    elif action == "set_if":
        cpu.if_ = event["value"] & 0x1F
    elif action == "set_ie":
        cpu.ie = event["value"]
    else:
        raise ValueError(f"unknown action {action}")


class Flat:
    """64 KiB of RAM, the scenario's in-cycle requests and its STOP answers."""

    def __init__(self, memory: bytearray, during: dict, answers: list | None) -> None:
        self.memory = memory
        self.during = during
        self.answers = answers
        self.asked = 0
        self.count = 0
        self.cpu: SM83 | None = None

    def _cycle(self) -> None:
        index = self.count
        self.count += 1
        for event in self.during.get(index, ()):
            act(self.cpu, event)

    def read_byte(self, address: int) -> int:
        self._cycle()
        return self.memory[address]

    def write_byte(self, address: int, value: int) -> None:
        self._cycle()
        self.memory[address] = value

    def idle(self, address: int) -> None:
        self._cycle()

    def stop(self, pending: bool) -> StopResult:
        answer = self.answers[self.asked % len(self.answers)]
        self.asked += 1
        return answer


def load(base: Path, blocks: list, memory: bytearray) -> None:
    for block in blocks:
        if "hex" in block:
            data = bytes.fromhex(block["hex"])
        else:
            file = Path(block["file"])
            data = (file if file.is_absolute() else base / file).read_bytes()
        for offset, byte in enumerate(data):
            memory[(block["address"] + offset) & 0xFFFF] = byte


def mocksoc_peek(soc: MockSoc):
    """MockSoc.read without its side effects (triggers, rows)."""

    def peek(address: int) -> int:
        if address < 0x8000:
            return soc.rom[address]
        if address == 0xFF0F:
            return 0xE0 | soc.cpu.if_
        if address == 0xFF44:
            return 0x90
        if address == 0xFF02:
            return 0x7F
        if 0xC000 <= address < 0xE000 or address >= 0xFF80:
            return soc.memory[address]
        return 0xFF

    return peek


def records(base: Path, scenario: dict):
    between: dict[int, list] = {}
    during: dict[int, list] = {}
    for event in scenario.get("events", ()):
        if "step" in event:
            between.setdefault(event["step"], []).append(event)
        else:
            during.setdefault(event["access"], []).append(event)
    state = scenario.get("state") or {}
    unknown = set(state) - STATE_FIELDS
    if unknown:
        raise ValueError(f"unknown state fields {sorted(unknown)}")
    track = scenario.get("accesses", True)
    host_kind = scenario.get("host", "flat")
    if host_kind == "flat":
        memory = bytearray(0x10000)
        if "fill" in scenario:
            block = bytes.fromhex(scenario["fill"])
            for i in range(0x10000):
                memory[i] = block[i % len(block)]
        load(base, scenario.get("memory", ()), memory)
        answers = None
        if scenario.get("stop") is not None:
            answers = [
                StopResult(StopAction(a["action"]), a.get("consume_operand", True))
                for a in scenario["stop"]
            ]
        host = Flat(memory, during, answers)
        cpu = SM83(
            host.read_byte, host.write_byte, host.idle, stop=host.stop if answers else None
        )
        host.cpu = cpu
        cpu.restore_state(CPUState(**state))
        peek = memory.__getitem__
        target = cpu
    elif host_kind == "harness":
        if scenario.get("events"):
            raise ValueError("harness scenarios take no events")
        harness = Harness((base / scenario["rom"]).read_bytes())
        cpu = harness.cpu
        if state:
            merged = {name: getattr(cpu.capture_state(), name) for name in STATE_FIELDS}
            merged.update(state)
            cpu.restore_state(CPUState(**merged))
        peek = harness.peek
        target = cpu
    elif host_kind == "mocksoc":
        if scenario.get("events"):
            raise ValueError("mocksoc scenarios take triggers")
        triggers = {t["address"]: (t["after"], t["mask"]) for t in scenario.get("triggers", ())}
        soc = MockSoc((base / scenario["rom"]).read_bytes(), triggers)
        cpu = soc.cpu
        cpu.restore_state(CPUState(**state))
        peek = mocksoc_peek(soc)
        target = cpu
    else:
        raise ValueError(f"unknown host {host_kind}")
    session = DebugSession(target, peek_byte=peek, track_accesses=track, history_limit=0)
    for sequence in range(scenario["steps"]):
        for event in between.get(sequence, ()):
            act(cpu, event)
        yield session.step()


def all_records(path: Path):
    document = json.loads(path.read_text())
    for scenario in document["runs"] if "runs" in document else [document]:
        yield from records(path.parent, scenario)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("scenario", type=Path)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    stream = args.out.open("w") if args.out else sys.stdout
    with stream:
        write_trace(all_records(args.scenario), stream)
    return 0


if __name__ == "__main__":
    sys.exit(main())
