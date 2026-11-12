#!/usr/bin/env python3
"""Write the lockstep scenarios (rungs 1 and 4).

Run it with the reference's interpreter: the die programs come from the reference's own
generator (``scripts/die_programs.py``). Hand-assembled scenarios go to
``conformance/scenarios/`` (committed: nothing third-party in them); those that name fetched
ROMs or reference-generated images go to ``external/scenarios/`` (not committed).

    make_scenarios.py [--blargg DIR] [--mooneye DIR] [--steps N]

Rung 1, programs: blargg ``cpu_instrs`` 01-11 and the Mooneye CPU ROMs in the test-ROM
harness, the first N boundaries each (default 200,000); the die all-opcodes ROM on the mock
SoC with its interrupt cases.

Rung 4, interrupts, HALT and STOP: every scenario of the reference's
``tests/test_special_states.py``; ``test_bus_schedule.py``'s idea widened to every opcode and
CB opcode from random states (IE, IF, IME, a pending EI, the HALT bug and in-cycle requests
included); the die-run HALT programs (``scripts/die_halt.py``'s 15 scenarios, and the
timer-woken ones of ``scripts/dmgsim_halt.py`` with the wake swept over 17 moments) on the
mock SoC; and an interrupt workout under seeded random requests between steps and inside
bus cycles, ``halt_sees`` and ``late`` included, with STOP answered every way.
"""

from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path

import sm83_python

ROOT = Path(__file__).resolve().parents[1]
COMMITTED = ROOT / "conformance" / "scenarios"
EXTERNAL = ROOT / "external" / "scenarios"
REFERENCE = Path(sm83_python.__file__).resolve().parents[2]
sys.path.insert(0, str(REFERENCE / "scripts"))

from die_compare import triggers_of  # noqa: E402
from die_programs import (  # noqa: E402
    Builder,
    add_halt_case,
    add_pending_halt_cases,
    add_timer_halt_cases,
    build,
    build_single,
)

MOONEYE = [
    "instr/daa", "bits/reg_f", "interrupts/ie_push", "if_ie_registers", "ei_sequence",
    "ei_timing", "rapid_di_ei", "intr_timing", "reti_intr_timing", "div_timing", "pop_timing",
]  # fmt: skip


def write(directory: Path, name: str, document: dict) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{name}.json").write_text(json.dumps(document, separators=(",", ":")))


def at(address: int, *data: int) -> dict:
    return {"address": address, "hex": bytes(data).hex()}


def run(memory=(), steps=1, events=(), stop=None, **state) -> dict:
    """A flat-RAM run; state defaults as the reference's ``host_at``: PC $0200, SP $C100."""
    state.setdefault("pc", 0x0200)
    state.setdefault("sp", 0xC100)
    scenario = {"host": "flat", "memory": list(memory), "state": state, "steps": steps}
    if events:
        scenario["events"] = list(events)
    if stop is not None:
        scenario["stop"] = stop
    return scenario


def irq(bit, *, step=None, access=None, halt_sees=False, late=False) -> dict:
    event = {"action": "irq", "bit": bit, "halt_sees": halt_sees, "late": late}
    event.update({"step": step} if step is not None else {"access": access})
    return event


# ----- rung 4: tests/test_special_states.py, scenario by scenario -------------------------


def special_states() -> list[dict]:
    runs = []
    for opcode in (0xD3, 0xDB, 0xDD, 0xE3, 0xE4, 0xEB, 0xEC, 0xED, 0xF4, 0xFC, 0xFD):
        runs.append(
            run([at(0x0200, opcode)], 6, [{"step": 1, "action": "set_if", "value": 1}],
                ie=0x01, if_=0x00, ime=True)
        )  # fmt: skip
    runs.append(run(steps=2, pc=0x1234, ie=0x04, if_=0x04, ime=True))
    for bit in range(5):
        runs.append(run(steps=2, ie=0x1F, if_=(1 << bit) | 0x10 if bit < 4 else 0x10, ime=True))
    for ie, ime in ((0x00, True), (0x01, False)):
        runs.append(run([at(0x0200, 0x00)], 2, ie=ie, if_=0x01, ime=ime))
    runs.append(run(steps=3, pc=0x0000, sp=0x0000, ie=0x01, if_=0x01, ime=True))
    runs.append(run(steps=2, pc=0x0100, sp=0x0000, ie=0x10, if_=0x11, ime=True))
    runs.append(run(steps=2, events=[irq(0, access=4)], ie=0x05, if_=0x04, ime=True))
    runs.append(run(steps=2, events=[irq(1, access=4, late=True)], ie=0x06, if_=0x04, ime=True))
    runs.append(run(steps=2, events=[irq(1, access=4, late=True)], ie=0x02, if_=0x02, ime=True))
    runs.append(run(steps=2, events=[irq(1, access=3, late=True)], ie=0x06, if_=0x04, ime=True))
    runs.append(run([at(0x0200, 0x00, 0x00)], 3, [irq(0, access=0)], ie=0x01, ime=True))
    runs.append(run([at(0x0200, 0xFB, 0x00, 0x00)], 4, ie=0x01, if_=0x01))
    runs.append(run([at(0x0200, 0xFB, 0xF3, 0x00)], 3, ie=0x01, if_=0x01))
    runs.append(run([at(0x0200, 0xF3)], 1, ime=True))
    runs.append(run([at(0x0200, 0xF3, 0x00)], 2, ei_pending=True))
    runs.append(run([at(0x0200, 0xD9), at(0xC000, 0x34, 0x12)], 2, sp=0xC000))
    runs.append(run([at(0x0200, 0x76, 0x00)], 7, [irq(0, step=4)], ie=0x01, ime=True))
    runs.append(run([at(0x0200, 0x76, 0x3C)], 3, [irq(0, step=1)], ie=0x01))
    runs.append(run([at(0x0200, 0x76)], 3, ie=0x01, if_=0x01, ei_pending=True))
    runs.append(run([at(0x0200, 0x76, 0x3C)], 4, [irq(0, access=0)], ie=0x01))
    runs.append(run([at(0x0200, 0x76, 0x3C)], 3, ie=0x01, if_=0x01))
    runs.append(run([at(0x0200, 0x76, 0x00)], 3, [irq(1, access=0, halt_sees=True)], ie=0x02,
                    ime=True))  # fmt: skip
    runs.append(run([at(0x0200, 0x00, 0x76, 0x3C)], 4, [irq(1, access=0, halt_sees=True)],
                    ie=0x02))  # fmt: skip
    runs.append(run([at(0x0200, 0x76, 0x3C)], 3, ie=0x01, if_=0x01))
    runs.append(run([at(0x0200, 0xFB, 0x76)], 4, ie=0x01, if_=0x01))
    runs.append(run([at(0x0200, 0x10, 0x00, 0x3C)], 6, [irq(4, step=4)]))
    for action, consume in (("stop", True), ("stop", False), ("halt", True), ("nop", False)):
        runs.append(
            run([at(0x0200, 0x10, 0x00)], 3, stop=[{"action": action, "consume_operand": consume}],
                ie=0x01, if_=0x01)
        )  # fmt: skip
    runs.append(run([at(0x0200, 0x77)], 1, a=0x15, h=0xFF, l=0xFF))
    runs.append(run([at(0x0200, 0x7E), at(0xFFFF, 0x00)], 1, ie=0xE5, h=0xFF, l=0xFF))
    runs.append(run([at(0x0200, 0xF1), at(0xC000, 0xFF, 0x12)], 1, sp=0xC000))
    return runs


# ----- rung 4: every opcode from random states ---------------------------------------------

STOP_ANSWERS = [
    {"action": "stop", "consume_operand": True},
    {"action": "stop", "consume_operand": False},
    {"action": "halt", "consume_operand": True},
    {"action": "halt", "consume_operand": False},
    {"action": "nop", "consume_operand": True},
    {"action": "nop", "consume_operand": False},
]


def random_run(rng: random.Random, opcode: int, prefixed: bool) -> dict:
    pc = rng.randrange(0x0200, 0xE000)
    code = [0xCB, opcode] if prefixed else [opcode]
    events = [
        irq(rng.randrange(5), access=rng.randrange(8), halt_sees=rng.random() < 0.5,
            late=rng.random() < 0.5)
        for _ in range(rng.choice((0, 0, 1, 2)))
    ]  # fmt: skip
    state = {
        "a": rng.randrange(256), "f": rng.randrange(16) << 4, "b": rng.randrange(256),
        "c": rng.randrange(256), "d": rng.randrange(256), "e": rng.randrange(256),
        "h": rng.randrange(256), "l": rng.randrange(256), "sp": rng.randrange(0x10000),
        "pc": pc, "ie": rng.randrange(256), "if_": rng.randrange(32),
        "ime": rng.random() < 0.5, "ei_pending": rng.random() < 0.3,
        "halt_bug": rng.random() < 0.1,
    }  # fmt: skip
    scenario = {
        "host": "flat",
        "fill": rng.randbytes(1024).hex(),
        "memory": [at(pc, *code)],
        "state": state,
        "steps": 3,
        "stop": [rng.choice(STOP_ANSWERS)],
    }
    if events:
        scenario["events"] = events
    return scenario


def opcode_runs() -> list[list[dict]]:
    rng = random.Random(83)
    runs = [
        random_run(rng, opcode, prefixed)
        for prefixed in (False, True)
        for opcode in range(256)
        for _ in range(4)
    ]
    return [runs[i : i + 128] for i in range(0, len(runs), 128)]


# ----- rung 4: the interrupt workout -------------------------------------------------------

WORKOUT = {
    0x0000: "c35001",  # JP $0150 (an IE-push cancellation lands here)
    0x0040: "c30004", 0x0048: "c31004", 0x0050: "c32004", 0x0058: "c33004", 0x0060: "c34004",
    0x0150: (
        "31f0df"    # LD SP,$DFF0
        "2100c0"    # LD HL,$C000
        "fb" "00"   # EI ; NOP
        "76" "04"   # HALT ; INC B          (IME = 1)
        "f3" "76"   # DI ; HALT             (IME = 0: wake without dispatch, or the HALT bug)
        "0c"        # INC C
        "fb" "76"   # EI ; HALT             (EI; HALT)
        "14"        # INC D
        "3e1f" "e0ff"  # LD A,$1F ; LDH (IE),A
        "1000"      # STOP
        "1c"        # INC E
        "cd0003"    # CALL $0300
        "f3" "76" "3c"  # DI ; HALT ; INC A
        "fb" "cb37"     # EI ; SWAP A
        "0800c1"    # LD ($C100),SP
        "e5" "d1"   # PUSH HL ; POP DE
        "f3" "310000"   # DI ; LD SP,$0000 (a dispatch now pushes PCH into IE)
        "fb" "00" "00"  # EI ; NOP ; NOP
        "31f0df"    # LD SP,$DFF0
        "3e1f" "e0ff"  # IE = $1F again
        "215001" "e5" "d9"  # LD HL,$0150 ; PUSH HL ; RETI (IME on at once, back to the top)
    ),
    0x0300: "f5" "3c" "27" "f1" "c9",  # PUSH AF ; INC A ; DAA ; POP AF ; RET
    0x0400: "f5" "34" "f1" "d9",  # PUSH AF ; INC (HL) ; POP AF ; RETI
    0x0410: "f5" "34" "f1" "fb" "c9",  # ... ; EI ; RET
    0x0420: "f5" "fb" "00" "f1" "d9",  # PUSH AF ; EI ; NOP (nested) ; POP AF ; RETI
    0x0430: "76" "00" "d9",  # HALT (IME = 0) ; NOP ; RETI
    0x0440: "c9",  # RET (IME stays off until the main loop's EI)
}  # fmt: skip


def workout(seed: int, steps: int = 20_000) -> dict:
    rng = random.Random(seed)
    memory = [{"address": a, "hex": h} for a, h in WORKOUT.items()]
    events = []
    for step in range(steps):
        roll = rng.random()
        if roll < 0.05:
            events.append(irq(rng.randrange(5), step=step, halt_sees=rng.random() < 0.3,
                              late=rng.random() < 0.3))  # fmt: skip
        elif roll < 0.07:
            events.append({"step": step, "action": "set_if", "value": rng.randrange(32)})
        elif roll < 0.075:
            events.append({"step": step, "action": "set_ie", "value": rng.randrange(256)})
    for access in sorted(rng.sample(range(steps * 3), steps // 10)):
        events.append(irq(rng.randrange(5), access=access, halt_sees=rng.random() < 0.5,
                          late=rng.random() < 0.5))  # fmt: skip
    rng.shuffle(stop := list(STOP_ANSWERS))
    return {
        "host": "flat",
        "memory": memory,
        "state": {"pc": 0x0150, "sp": 0xDFF0, "ie": 0x1F},
        "steps": steps,
        "events": events,
        "stop": stop,
    }


# ----- the die programs, on the mock SoC ---------------------------------------------------


def die_scenarios() -> dict[str, dict]:
    out = {}
    EXTERNAL.mkdir(parents=True, exist_ok=True)

    def soc(name: str, b: Builder, triggers: dict, steps: int) -> None:
        rom = EXTERNAL / f"{name}.gb"
        rom.write_bytes(bytes(b.image))
        out[name] = {
            "host": "mocksoc",
            "rom": rom.name,
            "state": {"pc": 0x0100},
            "steps": steps,
            "triggers": [
                {"address": a, "after": after, "mask": mask}
                for a, (after, mask) in sorted(triggers.items())
            ],
        }

    b = build()
    soc("die-all-opcodes", b, triggers_of(b.cases), 12_000)
    for ime in (False, True):
        for after in range(-1, 6):
            b = build_single(lambda b, i=ime, a=after: add_halt_case(b, i, a))
            soc(f"die-halt-ime{int(ime)}-if{after:+d}", b, triggers_of(b.cases), 400)
    b = build_single(add_pending_halt_cases)
    soc("die-halt-pending", b, triggers_of(b.cases), 400)
    timer = Builder()
    add_timer_halt_cases(timer)
    timer.case("end", [0x18, 0xFE])
    for after in range(8, 25):
        triggers = {
            c["address"] + c["tac_offset"]: (after, 0x04) for c in timer.cases if c.get("timer_halt")
        }  # fmt: skip
        soc(f"die-timer-halt-after{after}", timer, triggers, 6_000)
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--blargg", type=Path, help="gb-test-roms' cpu_instrs/individual")
    parser.add_argument("--mooneye", type=Path, help="the Mooneye build's acceptance/")
    parser.add_argument("--steps", type=int, default=200_000)
    args = parser.parse_args()
    write(COMMITTED, "special-states", {"runs": special_states()})
    for index, runs in enumerate(opcode_runs()):
        write(COMMITTED, f"opcodes-{index:02d}", {"runs": runs})
    for seed in range(8):
        write(COMMITTED, f"workout-{seed}", workout(seed))
    for name, scenario in die_scenarios().items():
        write(EXTERNAL, name, scenario)
    if args.blargg:
        for rom in sorted(args.blargg.glob("*.gb")):
            name = "blargg-" + rom.stem.split("-")[0]
            write(EXTERNAL, name, {"host": "harness", "rom": str(rom.resolve()), "steps": args.steps})
    if args.mooneye:
        for test in MOONEYE:
            name = "mooneye-" + test.replace("/", "-")
            rom = (args.mooneye / f"{test}.gb").resolve()
            write(EXTERNAL, name, {"host": "harness", "rom": str(rom), "steps": args.steps})
    print(f"scenarios in {COMMITTED} and {EXTERNAL}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
