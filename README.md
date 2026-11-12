# sm83-rust

A Rust Sharp SM83 (Game Boy CPU) core transcribed from
[sm83-python](https://github.com/alewman/sm83-python) at `95f2b8b` and proven equivalent to
it at every M-cycle: every bus access in order (address, value, direction, the IDU, the
idle/locked/halted cycles that drive `$0000` or `{PCH,$00}`), boundary kinds, and all of
`CpuState` (`halt_dispatch`, `stop_if` and the in-step `pending_at_fetch` and `unsampled`
included). It is the CPU of [gameboy-rust](https://github.com/alewman/gameboy-rust). The crate
is `sm83`; it is not published to crates.io (use it by path or git).

```rust
use sm83::{Bus, Sm83};

struct Ram(Vec<u8>);
impl Bus for Ram {
    fn read(&mut self, address: u16, _: &mut Sm83) -> u8 { self.0[address as usize] }
    fn write(&mut self, address: u16, value: u8, _: &mut Sm83) { self.0[address as usize] = value; }
}

let mut ram = Ram(vec![0; 0x10000]);
let mut cpu = Sm83::new();
let m_cycles = cpu.step(&mut ram); // one bus call per M-cycle, internal cycles included
```

```sh
cargo test --release      # the transcribed special-state tests and the example above
```

## Status

All five rungs pass and 12 of 12 seeded faults are caught; no divergence from sm83-python
was found. Results and exact commands: [docs/status.md](docs/status.md).

| Rung | What | Result |
| --- | --- | --- |
| 1 | Trace lockstep on programs: blargg `cpu_instrs` 01-11 and 11 Mooneye CPU ROMs in the harness, the die all-opcodes ROM on the mock SoC | 23 of 23 traces identical |
| 2 | SingleStepTests `sm83`, every cycle | 498,000 of 498,000 cases |
| 3 | blargg `cpu_instrs` and the Mooneye CPU ROMs, natively | 22 of 22 as expected (blargg 02 fails as the reference does: the harness has no timer) |
| 4 | Interrupts, HALT and STOP in lockstep: the reference's special-state scenarios, every opcode from random states, seeded interrupt workouts, the die HALT programs | 57 of 57 identical |
| 5 | MAME replay: the same results as the reference's `replay` on the same MAME 0.285 traces | 16 of 16 identical |
| faults | One-line faults seeded into the crate, one at a time (`scripts/seeded_faults.py`) | 12 of 12 caught |

## How a board drives it

The same contract as sm83-python's ("The bus contract" in its `docs/handoff-brief.md`), which
gameboy-python's `board.py` relies on:

- The board owns memory and every device and implements `Bus`: `read`, `write`, and
  optionally `idle(address)` (internal cycles: the address the CPU drives, for the OAM bug)
  and `stop(pending) -> StopResult` (the STOP decision: `Stop`, `Halt` or `Nop`, and whether
  the operand byte is consumed; a speed switch happens inside the hook, which then answers
  `Halt`). It advances its devices **inside** each call (4 T-cycles, or 2 in CGB double
  speed); `step()`'s return value is for counting, never to be added on top.
- Each call gets `&mut Sm83`, so a device can raise an interrupt in the cycle it happens:
  `cpu.request_interrupt(bit, halt_sees, late)`. `halt_sees`: raised early enough in this
  M-cycle for a HALT fetched now to see it (STAT); `late`: raised after the dispatch's
  low-byte push sampled IF (an HBlank STAT from the cycle's second dot). The call also sees
  `cpu.idu` (+1/-1/0, the IDU on this cycle's address), `cpu.pc`, `cpu.halted`, `cpu.ime`,
  `cpu.ie`, and may read or write `cpu.if_` (the board maps `$FF0F` to it; the core owns
  `$FFFF` and answers it itself) or any other field, as `board.py` does.
- A halted, stopped or locked CPU makes one idle cycle per `step()`; the board loops. The
  board can wake STOP itself (`cpu.stopped = false`) or set `cpu.stop_if`.
- `capture_state()` / `restore_state()` are the save state (`CpuState`); `reset()` is the
  reset line. `sm83::trace` writes the family's JSON Lines traces (`step_record`,
  `Tracked`), which gameboy-rust uses for its own lockstep against gameboy-python.

## Layout

Each `src/*.rs` file is the transcription of one `sm83_python/*.py` module, with the same
handler names, the same explicit dispatch (the reference's bit-pattern rules, built into a
256-entry table at compile time) and the hardware comments carried over: `core`
(`_core.py` and the `SM83` fields), `alu`, `flags`, `loads`, `rotate`, `control`,
`opcodes` (generated from the reference's generated table by `scripts/gen_opcodes.py`),
`dispatch`, `cpu`, `stop`, `state`, `trace`, `harness`, `replay`. Where Rust forced a
difference, the module header says so; the one that matters is the bus (a host is passed to
`step` and each call also gets `&mut Sm83`, where a Python board reaches the CPU through
its own reference). Not transcribed: `debug.py`, `console.py`, `disasm.py` and
`replay.py`'s CLI; a trace written here carries each instruction's address and bytes, and
the reference's reader disassembles them.

Binaries (the ladder's tools): `trace` (lockstep scenarios, [conformance/](conformance/README.md)),
`sst` (SingleStepTests), `roms` (test ROMs in the harness), `replay` (MAME traces).

## Reproducing the ladder

`PYTHON=pypy3 scripts/fetch_sm83python.sh` puts the reference at its pin in `external/` (a
local clone can be given as the second argument), `scripts/fetch_references.sh` fetches
SingleStepTests, blargg and Mooneye (SHA-256 checked), then run the `scripts/rung*.sh`
scripts. Rung 5 needs MAME traces (sm83-python's `scripts/mame_trace.py --io`) in
`external/mame_traces/`. `scripts/seeded_faults.py` builds the crate with one fault at a time
and checks that the ladder catches each.

CI (`.github/workflows/ci.yml`, main and pull requests) runs `cargo fmt --check`,
`cargo clippy -D warnings`, `cargo test`, a rung-2 sample and rung 3, all from public
sources. Rungs 1, 4 and 5 need the reference (and rung 5 MAME and commercial ROMs) and are
run by hand.

## Licence and credits

MIT (`LICENSE`). Nothing third-party is committed: test vectors and ROMs are fetched by
script; `conformance/scenarios/` are generated here by `scripts/make_scenarios.py`.

The referees: [SingleStepTests](https://github.com/SingleStepTests/sm83) (`sm83`),
blargg's `gb-test-roms`, the [Mooneye Test Suite](https://github.com/Gekkio/mooneye-test-suite)
and MAME's Game Boy driver for the replay traces.
