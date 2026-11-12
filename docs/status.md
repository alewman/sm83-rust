# Status (2026-11-12)

The crate is complete against its brief (docs/handoff-brief.md): every module of
sm83-python `95f2b8b` transcribed, all five rungs pass, seeded faults all caught.
Run on Linux x86_64, rustc 1.93.1, reference on PyPy 7.3.20 (Python 3.11).

## What passed

| Rung | Result | Command |
| --- | --- | --- |
| 1 | 23 of 23 traces identical: blargg `cpu_instrs` 01-11 and 11 Mooneye CPU ROMs in the harness (first 200,000 boundaries each), die all-opcodes ROM on the mock SoC (12,000 boundaries) | `scripts/rung1.sh` |
| 2 | SingleStepTests sm83: 498 files, 498,000 cases, 0 failed | `scripts/rung2.sh` |
| 3 | 22 of 22: blargg 01, 03-11 "Passed", 02 "Failed #4" (no timer, as documented); Mooneye daa, reg_f, ie_push, if_ie_registers, ei_sequence, ei_timing, rapid_di_ei, intr_timing, reti_intr_timing, div_timing, pop_timing pass. Instruction and M-cycle counts equal to the reference harness's (checked on 01, 02, 06, daa, pop_timing) | `scripts/rung3.sh` |
| 4 | 57 of 57 scenarios identical: test_special_states.py's scenarios (35 runs); every opcode and CB opcode from 4 random states each (random IE/IF/IME/EI-pending/HALT-bug, in-cycle `halt_sees`/`late` requests, STOP answers), 16 files; 8 seeded interrupt workouts of 20,000 boundaries (per workout about 1,600-2,100 dispatches, 4,500-5,200 halted cycles, 220-250 HALT bugs, 34-47 EI;HALT dispatches, 11-15 STOPs); the die HALT entry programs (15) and the timer-woken HALT ROM at 17 wake moments on the mock SoC | `scripts/rung4.sh` |
| 5 | 16 of 16 replays identical (strict and report) on fresh MAME 0.285 `--io` traces: blargg 03 (732,726 lines), 06 (625,119), halt_bug (both stop at the same divergence, line 195,426, PC C07E vs MAME C07D: MAME fails halt_bug), Tetris (8.40 M), Kirby's Dream Land (7.51 M), Castlevania (1.78 M), Dr. Mario (1.68 M), Super Mario Land (1.49 M) | `scripts/rung5.sh` |
| faults | 12 of 12 seeded one-line faults caught (late, halt_sees, JR address, halted address, HALT length, EI delay, DAA, IE write order, EI;HALT PC, stop_if, ADD SP flags, dispatch M2 address). A first DAA mutation was equivalent (adding $60 keeps the low nibble) and was replaced | `scripts/seeded_faults.py` |

Rust tests (`cargo test`): `tests/special_states.rs` (17 tests transcribed from the
reference's test_special_states.py) and the doc example. They catch 9 of the 12 faults on
their own, which matters because CI cannot run the lockstep rungs.

No divergence from sm83-python was found; nothing suggests the reference is wrong.

## Setup, exact commands

```sh
PYTHON=~/.local/opt/pypy/bin/pypy3 scripts/fetch_sm83python.sh "" /data/emu/sm83-python
scripts/fetch_references.sh
JOBS=10 scripts/rung1.sh
scripts/rung2.sh
scripts/rung3.sh
JOBS=10 scripts/rung4.sh
# rung 5 traces, recorded with the reference's own recorder (MAME 0.285 at /usr/games/mame):
#   python3 -I /data/emu/sm83-python/scripts/mame_trace.py gameboy ROM --io --seconds 20 \
#       --out external/mame_traces/NAME
scripts/rung5.sh
python3 scripts/seeded_faults.py
```

## Open

- Local `cargo clippy` is a 2024 snap build that refuses the crate (`requires rustc 1.85`);
  clippy runs in CI only.
- CI runs fmt, clippy, tests, a rung-2 sample and rung 3. Rungs 1, 4 and 5 need the
  private reference (and MAME, and commercial ROMs for rung 5) and run by hand.
- The MAME traces and the commercial ROMs are local only (external/, not committed).
