# sm83-rust

A Rust Sharp SM83 (Game Boy CPU) core transcribed from
[sm83-python](https://github.com/alewman/sm83-python) at `95f2b8b` (the revision
gameboy-python pins) and proven equivalent to it at every M-cycle: every bus access in
order (address, value, direction, the IDU, the idle/locked/halted cycles that drive `$0000`
or `{PCH,$00}`), boundary kinds, and all of `CpuState` (`halt_dispatch`, `stop_if` and the
in-step `pending_at_fetch` and `unsampled` included). It is the CPU of the Rust Game Boy
board to come (`gameboy-rust`). The crate is `sm83`.

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

## How a board drives it

The same contract as sm83-python's (its `docs/handoff-brief.md` "The bus contract"), which
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
  `Tracked`), for gameboy-rust's own lockstep against gameboy-python.

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

## Certification

Ladder status and the exact commands are in [docs/status.md](docs/status.md); the brief is
[docs/handoff-brief.md](docs/handoff-brief.md).

| Rung | What | Script |
| --- | --- | --- |
| 1 | Trace lockstep on programs: blargg `cpu_instrs` 01-11 and 11 Mooneye CPU ROMs in the harness, the die all-opcodes ROM on the mock SoC | `rung1.sh` |
| 2 | SingleStepTests `sm83`, 498,000 cases, every cycle | `rung2.sh` |
| 3 | blargg `cpu_instrs` 01, 03-11 and the Mooneye CPU ROMs natively | `rung3.sh` |
| 4 | Interrupts, HALT and STOP in lockstep: `test_special_states.py`'s scenarios, every opcode from random states, a seeded interrupt workout, the die HALT programs | `rung4.sh` |
| 5 | MAME replay: identical results to the reference's `replay` on the same traces | `rung5.sh` |

Reproducing: `PYTHON=pypy3 scripts/fetch_sm83python.sh` (the reference at its pin, private:
needs GitHub credentials or a local clone as the second argument),
`scripts/fetch_references.sh` (SingleStepTests, blargg, Mooneye, SHA-256 checked), then
the rung scripts. Rung 5 needs MAME traces (sm83-python's `scripts/mame_trace.py --io`)
in `external/mame_traces/`. `scripts/seeded_faults.py` builds the crate with one fault at
a time and checks that the ladder catches each.

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, `cargo clippy -D warnings`,
`cargo test` (the special-state tests, transcribed), a rung-2 sample and rung 3; the
lockstep rungs need the private reference and run by hand.

## Licence

MIT. Nothing third-party is committed: test vectors and ROMs are fetched by script.
