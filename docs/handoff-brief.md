# Handoff brief: sm83-rust

## The request

Aubrey, 2026-11-12, after gameboy-python reached Mealybug DMG 24/24 and
Shootout 256/258: start the family's Rust path for the Game Boy
(sm83-rust → gameboy-rust → -fast → a play-gb browser page), in parallel
with the Python board's remaining accuracy work. This brief is the first
step only: the CPU core.

## What it is

A Rust Sharp SM83 core transcribed from sm83-python
(`/data/emu/sm83-python`, GitHub `alewman/sm83-python`, PRIVATE) at
**`95f2b8b`** (master, the merge of PR #2), the revision gameboy-python pins.
It must be equivalent to that revision at every M-cycle: every bus access
in order (address, value, direction, the idle/locked/halted cycles that
drive `$0000` or `{PCH,$00}`), boundary kinds, and all of `CPUState`
(including `halt_dispatch`, `stop_if`, `pending_at_fetch`).

Crate name `sm83`. Repo `/data/emu/sm83-rust`. Create a **private** GitHub
remote `alewman/sm83-rust` when the ladder passes (same visibility as its
reference) and push; minimal CI per the family rule (main + PRs, concurrency
cancel; fmt, clippy -D warnings, test, rung 1, a rung-2 sample).

## What to transcribe

From `src/sm83_python/` at `95f2b8b`, module for module, same names, the
same explicit dispatch, hardware comments carried over: `_core.py`,
`_alu.py`, `_flags.py`, `_loads.py`, `_rotate.py`, `_control.py`,
`_opcodes.py`, `_dispatch.py`, `cpu.py`, `stop.py`, `state.py`, `trace.py`
(same JSON Lines schema, `docs/trace-schema.md`), and `harness.py` if the
test ROM rungs need it. The board hooks gameboy-python uses are part of the
core and must be transcribed exactly: `request_interrupt(bit, halt_sees=...,
late=...)`, the `unsampled` M5 dispatch state, the HALT / HALT-bug /
EI;HALT `halt_dispatch` rules, STOP. Read `docs/timing.md`,
`docs/undocumented-behavior.md` and `docs/validation.md` first; and look at
how gameboy-python (`/data/emu/gameboy-python/src/gameboy_python/board.py`)
drives the core, because gameboy-rust will drive this crate the same way.

Not transcribed: `debug.py`, `console.py`, `disasm.py`, `replay.py`'s CLI
(a trace's `instruction` text may be null; comparison ignores it).

## The ladder

Each rung has a script in `scripts/` and passes only when its output says so.

1. **Trace lockstep**: the same programs and scenarios on both cores
   (sm83-python's own trace writer and divergence finder), identical.
2. **SingleStepTests `sm83`**: all 498,000 cases, registers, RAM and every
   cycle's bus activity. All pass.
3. **blargg cpu_instrs (01, 03-11) and the Mooneye CPU ROMs** sm83-python
   runs, natively, binaries fetched by script with SHA-256 as sm83-python
   does.
4. **Interrupts, HALT and STOP in lockstep**: the scenarios of
   `tests/test_special_states.py`, `test_bus_schedule.py` and the die-run
   programs (`scripts/die_programs.py`) incl. `halt_sees` and `late`
   requests, traced on both cores and diffed. Identical.
5. **MAME replay** (`tests/mame_traces`, sm83-python's replay format) if it
   is cheap to drive: identical to sm83-python over the same traces.

Seed faults afterwards (as 6502-rust did: ~10 single-line mutations) and
report how many the ladder catches; add a case for any it misses.

## Working rules

- Do not modify sm83-python. If it looks wrong, stop and show the
  divergence; the vectors, test ROMs and die runs are the authority.
- Transcribe; do not redesign. Where Rust forces a difference, say so in the
  module header. Correct, not fast (speed is a later -fast fork).
- Model the repo on `/data/emu/6502-rust` and `/data/emu/z80-rust`
  (layout, scripts/rungN.sh, README, docs/handoff-brief.md copy of this file).
- Never copy GPL code; nothing third-party committed (vectors, ROMs fetched).
- `nice`, at most about 12 processes (the machine is shared). Local
  `cargo clippy` may be an old snap nightly that cannot build; CI runs clippy.
- No `rm -rf`, `sed -i` or `ln -sfn` one-liners; use the Edit tool and new
  directories instead.
- Commit locally at good points without asking. Commit trailer:
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Leave `docs/status.md` with what passed, what is open and the exact
  commands, when you stop.
