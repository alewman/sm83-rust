# Lockstep scenarios

A scenario is run by both cores, `scripts/reference_trace.py` on sm83-python and
`target/release/trace` here, and the two version 1 traces (sm83-python
`docs/trace-schema.md`) are compared with the reference's own
`first_trace_divergence` (`scripts/compare_traces.py`). `scripts/lockstep.py` does all
three for a list of scenarios.

`scenarios/` holds the hand-assembled ones (written by `scripts/make_scenarios.py`; nothing
third-party). Scenarios that name fetched ROMs (blargg, Mooneye) or images built by the
reference's own die-program generator go to `external/scenarios/`, not committed.

## Format

A JSON object, or `{"runs": [object, ...]}` whose traces are concatenated.

| Key | Meaning |
| --- | --- |
| `host` | `flat` (default): 64 KiB of RAM. `harness`: the test-ROM harness (`sm83::harness`, the reference's `Harness`). `mocksoc`: the die testbench's memory map (the reference's `scripts/die_compare.py` `MockSoc`) |
| `memory` | `flat`: blocks `{"address", "hex"}` or `{"address", "file"}` loaded into RAM, after `fill` |
| `fill` | `flat`: hex bytes repeated over all 64 KiB first |
| `rom` | `harness`, `mocksoc`: the ROM image, relative to the scenario's directory or absolute |
| `state` | `CPUState` fields (`a f b c d e h l sp pc ie if_ ime ei_pending halted halt_bug halt_dispatch stopped locked stop_if`); the rest zero (`harness`: the post-boot state) |
| `steps` | boundaries to run and record |
| `events` | `flat`: `{"step": n}` before step n or `{"access": n}` inside the n-th bus call (0-based, every M-cycle counted), with `action` `irq` (`bit`, `halt_sees`, `late`), `set_if` or `set_ie` (`value`) |
| `stop` | `flat`: STOP answers `{"action": "stop"/"halt"/"nop", "consume_operand"}`, used in turn; absent: the core's default |
| `triggers` | `mocksoc`: `{"address", "after", "mask"}`: a fetch at `address` raises the IF bits of `mask` `after` M-cycles later |
| `accesses` | record every M-cycle (default true) |
