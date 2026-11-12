//! Run a scenario and write its boundary trace (rungs 1 and 4).
//!
//!     trace SCENARIO.json [--out TRACE.jsonl]
//!
//! The scenario format (conformance/README.md) is read the same way by
//! `scripts/reference_trace.py`, which runs it on sm83-python; the two traces are compared
//! with the reference's `first_trace_divergence`. Three hosts:
//!
//! - `flat`: 64 KiB of RAM (the reference's test hosts), with scheduled interrupt requests
//!   between steps and inside bus cycles (`halt_sees`, `late`), IF writes, and STOP answers;
//! - `harness`: the test-ROM harness (`sm83::harness`), as the reference's `Harness`;
//! - `mocksoc`: the die testbench's memory map with fetch-triggered interrupt requests, a
//!   transcription of the reference's `scripts/die_compare.py` `MockSoc`.

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::Deserialize;
use sm83::harness::Harness;
use sm83::trace::step_record;
use sm83::{Bus, CpuState, Sm83, StopAction, StopResult};

#[derive(Deserialize, Clone)]
struct Block {
    address: u16,
    #[serde(default)]
    hex: Option<String>,
    #[serde(default)]
    file: Option<String>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
struct State {
    a: Option<u8>,
    f: Option<u8>,
    b: Option<u8>,
    c: Option<u8>,
    d: Option<u8>,
    e: Option<u8>,
    h: Option<u8>,
    l: Option<u8>,
    sp: Option<u16>,
    pc: Option<u16>,
    ie: Option<u8>,
    if_: Option<u8>,
    ime: Option<bool>,
    ei_pending: Option<bool>,
    halted: Option<bool>,
    halt_bug: Option<bool>,
    halt_dispatch: Option<bool>,
    stopped: Option<bool>,
    locked: Option<bool>,
    stop_if: Option<u8>,
}

impl State {
    fn over(&self, base: CpuState) -> CpuState {
        CpuState {
            a: self.a.unwrap_or(base.a),
            f: self.f.unwrap_or(base.f),
            b: self.b.unwrap_or(base.b),
            c: self.c.unwrap_or(base.c),
            d: self.d.unwrap_or(base.d),
            e: self.e.unwrap_or(base.e),
            h: self.h.unwrap_or(base.h),
            l: self.l.unwrap_or(base.l),
            sp: self.sp.unwrap_or(base.sp),
            pc: self.pc.unwrap_or(base.pc),
            ie: self.ie.unwrap_or(base.ie),
            if_: self.if_.unwrap_or(base.if_),
            ime: self.ime.unwrap_or(base.ime),
            ei_pending: self.ei_pending.unwrap_or(base.ei_pending),
            halted: self.halted.unwrap_or(base.halted),
            halt_bug: self.halt_bug.unwrap_or(base.halt_bug),
            halt_dispatch: self.halt_dispatch.unwrap_or(base.halt_dispatch),
            stopped: self.stopped.unwrap_or(base.stopped),
            locked: self.locked.unwrap_or(base.locked),
            stop_if: self.stop_if.unwrap_or(base.stop_if),
        }
    }
}

#[derive(Deserialize, Clone)]
struct Event {
    #[serde(default)]
    step: Option<u64>,
    #[serde(default)]
    access: Option<u64>,
    action: String,
    #[serde(default)]
    bit: u8,
    #[serde(default)]
    halt_sees: bool,
    #[serde(default)]
    late: bool,
    #[serde(default)]
    value: u8,
}

#[derive(Deserialize, Clone)]
struct Answer {
    action: String,
    #[serde(default = "default_true")]
    consume_operand: bool,
}

#[derive(Deserialize, Clone)]
struct Trigger {
    address: u16,
    after: u64,
    mask: u8,
}

#[derive(Deserialize, Clone)]
struct Scenario {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default)]
    memory: Vec<Block>,
    #[serde(default)]
    fill: Option<String>,
    #[serde(default)]
    rom: Option<String>,
    #[serde(default)]
    state: Option<State>,
    steps: u64,
    #[serde(default)]
    events: Vec<Event>,
    #[serde(default)]
    stop: Option<Vec<Answer>>,
    #[serde(default)]
    triggers: Vec<Trigger>,
    #[serde(default = "default_true")]
    accesses: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum File {
    Runs { runs: Vec<Scenario> },
    One(Scenario),
}

fn default_host() -> String {
    "flat".into()
}

fn default_true() -> bool {
    true
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

fn resolve(base: &Path, name: &str) -> std::path::PathBuf {
    let path = Path::new(name);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn act(cpu: &mut Sm83, event: &Event) {
    match event.action.as_str() {
        "irq" => cpu.request_interrupt(event.bit, event.halt_sees, event.late),
        "set_if" => cpu.if_ = event.value & 0x1F,
        "set_ie" => cpu.ie = event.value,
        other => panic!("unknown action {other}"),
    }
}

/// 64 KiB of RAM, the scenario's in-cycle requests and its STOP answers.
struct Flat {
    memory: Vec<u8>,
    count: u64,
    during: HashMap<u64, Vec<Event>>,
    answers: Option<Vec<StopResult>>,
    asked: usize,
}

impl Flat {
    fn cycle(&mut self, cpu: &mut Sm83) {
        let index = self.count;
        self.count += 1;
        if let Some(events) = self.during.get(&index) {
            for event in events {
                act(cpu, event);
            }
        }
    }
}

impl Bus for Flat {
    fn read(&mut self, address: u16, cpu: &mut Sm83) -> u8 {
        self.cycle(cpu);
        self.memory[address as usize]
    }

    fn write(&mut self, address: u16, value: u8, cpu: &mut Sm83) {
        self.cycle(cpu);
        self.memory[address as usize] = value;
    }

    fn idle(&mut self, _address: u16, cpu: &mut Sm83) {
        self.cycle(cpu);
    }

    fn stop(&mut self, _pending: bool, _cpu: &mut Sm83) -> StopResult {
        match &self.answers {
            None => StopResult::new(StopAction::Stop),
            Some(answers) => {
                let answer = answers[self.asked % answers.len()];
                self.asked += 1;
                answer
            }
        }
    }
}

/// The die testbench's memory map: ROM, WRAM, HRAM, IF, LY and SC; the rest floats high
/// (`die_compare.MockSoc`).
struct MockSoc {
    rom: Vec<u8>,
    memory: Vec<u8>,
    rows: u64,
    /// fetch address -> (cycles later, IF mask) to raise IF
    triggers: HashMap<u16, (u64, u8)>,
    /// (row index, IF mask) still to raise
    due: Vec<(u64, u8)>,
}

impl MockSoc {
    /// Log one M-cycle and raise any interrupt scheduled for it.
    fn row(&mut self, cpu: &mut Sm83) {
        let index = self.rows;
        self.rows += 1;
        let mut remaining = Vec::with_capacity(self.due.len());
        let due = std::mem::take(&mut self.due);
        for (at, mask) in due {
            if at == index {
                for bit in 0..5 {
                    if (mask >> bit) & 1 != 0 {
                        cpu.request_interrupt(bit, false, false);
                    }
                }
            } else {
                remaining.push((at, mask));
            }
        }
        self.due = remaining;
    }

    fn peek(&self, address: u16, if_: u8) -> u8 {
        if address < 0x8000 {
            self.rom[address as usize]
        } else if address == 0xFF0F {
            0xE0 | if_
        } else if address == 0xFF44 {
            0x90
        } else if address == 0xFF02 {
            0x7F
        } else if (0xC000..0xE000).contains(&address) || address >= 0xFF80 {
            self.memory[address as usize]
        } else {
            0xFF
        }
    }
}

impl Bus for MockSoc {
    fn read(&mut self, address: u16, cpu: &mut Sm83) -> u8 {
        let value = self.peek(address, cpu.if_);
        if cpu.idu == 1 {
            if let Some(&(after, mask)) = self.triggers.get(&address) {
                // a fetch that starts a countdown
                self.due.push((self.rows + after, mask));
            }
        }
        self.row(cpu);
        value
    }

    fn write(&mut self, address: u16, value: u8, cpu: &mut Sm83) {
        if address == 0xFF0F {
            cpu.if_ = value & 0x1F;
        } else if (0xC000..0xE000).contains(&address) || address >= 0xFF80 {
            self.memory[address as usize] = value;
        }
        self.row(cpu);
    }

    fn idle(&mut self, _address: u16, cpu: &mut Sm83) {
        self.row(cpu);
    }
}

fn load(base: &Path, blocks: &[Block], memory: &mut [u8]) {
    for block in blocks {
        let data = match (&block.hex, &block.file) {
            (Some(text), _) => hex(text),
            (None, Some(file)) => std::fs::read(resolve(base, file)).expect("read memory file"),
            _ => panic!("a memory block needs hex or file"),
        };
        for (offset, byte) in data.iter().enumerate() {
            memory[(block.address as usize + offset) & 0xFFFF] = *byte;
        }
    }
}

fn run(base: &Path, scenario: &Scenario, out: &mut impl Write, sequence: &mut u64) {
    let mut between: HashMap<u64, Vec<Event>> = HashMap::new();
    let mut during: HashMap<u64, Vec<Event>> = HashMap::new();
    for event in &scenario.events {
        match (event.step, event.access) {
            (Some(step), _) => between.entry(step).or_default().push(event.clone()),
            (None, Some(access)) => during.entry(access).or_default().push(event.clone()),
            _ => panic!("an event needs step or access"),
        }
    }
    let track = scenario.accesses;
    let mut emit = |line: String| {
        out.write_all(line.as_bytes()).unwrap();
        out.write_all(b"\n").unwrap();
    };
    match scenario.host.as_str() {
        "flat" => {
            let mut memory = vec![0u8; 0x10000];
            if let Some(fill) = &scenario.fill {
                let block = hex(fill);
                for (i, byte) in memory.iter_mut().enumerate() {
                    *byte = block[i % block.len()];
                }
            }
            load(base, &scenario.memory, &mut memory);
            let answers = scenario.stop.as_ref().map(|answers| {
                answers
                    .iter()
                    .map(|a| {
                        let action = match a.action.as_str() {
                            "stop" => StopAction::Stop,
                            "halt" => StopAction::Halt,
                            "nop" => StopAction::Nop,
                            other => panic!("unknown stop action {other}"),
                        };
                        StopResult::with_operand(action, a.consume_operand)
                    })
                    .collect()
            });
            let mut host = Flat {
                memory,
                count: 0,
                during,
                answers,
                asked: 0,
            };
            let mut cpu = Sm83::new();
            let state = scenario.state.clone().unwrap_or_default();
            cpu.restore_state(&state.over(CpuState::default()));
            for step in 0..scenario.steps {
                if let Some(events) = between.get(&step) {
                    for event in events {
                        act(&mut cpu, event);
                    }
                }
                let pc = cpu.pc;
                let bytes: Vec<u8> = (0..3)
                    .map(|i| host.memory[pc.wrapping_add(i) as usize])
                    .collect();
                let peek = move |a: u16| bytes[a.wrapping_sub(pc) as usize];
                let record = step_record(&mut cpu, &mut host, Some(&peek), *sequence, track);
                *sequence += 1;
                emit(record.to_line());
            }
        }
        "harness" => {
            assert!(
                scenario.events.is_empty(),
                "harness scenarios take no events"
            );
            let rom = std::fs::read(resolve(base, scenario.rom.as_ref().expect("rom"))).unwrap();
            let mut harness = Harness::new(&rom, None);
            if let Some(state) = &scenario.state {
                let merged = state.over(harness.cpu.capture_state());
                harness.cpu.restore_state(&merged);
            }
            for _ in 0..scenario.steps {
                let board = &harness.board;
                let pc = harness.cpu.pc;
                // peek only reads the instruction's bytes: copy them before the step
                let bytes: Vec<u8> = (0..3).map(|i| board.peek(pc.wrapping_add(i))).collect();
                let peek = move |a: u16| bytes[a.wrapping_sub(pc) as usize];
                let record = step_record(
                    &mut harness.cpu,
                    &mut harness.board,
                    Some(&peek),
                    *sequence,
                    track,
                );
                *sequence += 1;
                emit(record.to_line());
            }
        }
        "mocksoc" => {
            assert!(
                scenario.events.is_empty(),
                "mocksoc scenarios take triggers"
            );
            let rom = std::fs::read(resolve(base, scenario.rom.as_ref().expect("rom"))).unwrap();
            let mut soc = MockSoc {
                rom,
                memory: vec![0; 0x10000],
                rows: 0,
                triggers: scenario
                    .triggers
                    .iter()
                    .map(|t| (t.address, (t.after, t.mask)))
                    .collect(),
                due: Vec::new(),
            };
            let mut cpu = Sm83::new();
            let state = scenario.state.clone().unwrap_or_default();
            cpu.restore_state(&state.over(CpuState::default()));
            for _ in 0..scenario.steps {
                let pc = cpu.pc;
                let bytes: Vec<u8> = (0..3)
                    .map(|i| soc.peek(pc.wrapping_add(i), cpu.if_))
                    .collect();
                let peek = move |a: u16| bytes[a.wrapping_sub(pc) as usize];
                let record = step_record(&mut cpu, &mut soc, Some(&peek), *sequence, track);
                *sequence += 1;
                emit(record.to_line());
            }
        }
        other => panic!("unknown host {other}"),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut scenario_path = None;
    let mut out_path = None;
    while let Some(arg) = args.next() {
        if arg == "--out" {
            out_path = args.next();
        } else {
            scenario_path = Some(arg);
        }
    }
    let scenario_path = scenario_path.expect("usage: trace SCENARIO.json [--out TRACE.jsonl]");
    let text = std::fs::read_to_string(&scenario_path).expect("read scenario");
    let file: File = serde_json::from_str(&text).expect("parse scenario");
    let base = Path::new(&scenario_path)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let stream: Box<dyn Write> = match out_path {
        Some(path) => Box::new(std::fs::File::create(path).expect("create trace")),
        None => Box::new(std::io::stdout()),
    };
    let mut out = BufWriter::new(stream);
    let mut sequence = 0;
    match file {
        File::Runs { runs } => {
            for scenario in &runs {
                run(&base, scenario, &mut out, &mut sequence);
            }
        }
        File::One(scenario) => run(&base, &scenario, &mut out, &mut sequence),
    }
    out.flush().unwrap();
}
