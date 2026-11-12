//! Replay a MAME 0.285 trace of real Game Boy code on the core (`replay.py`; validation.md
//! rung 4).
//!
//! MAME is an emulator-derived detector (docs/mame-oracle.md). Real programs read I/O
//! registers whose values come from a board the core does not have, so the replay is
//! board-free: every read of $FF00-$FF7F and $FFFF is answered from MAME's own log
//! (`scripts/mame_trace.py --io`), in order, and writes to those registers are compared by
//! omission (not obeyed). The core runs the cartridge ROM and RAM itself, starting from the
//! registers MAME has at PC = $0100.
//!
//! After each instruction the replay compares `PC A F B C D E H L SP IE` with MAME's next
//! line, and the M-cycles the step took with the difference of MAME's `totalcycles` divided by
//! 4 (not across a HALT, whose length the board decides). IF is the board's: it is set from
//! MAME's line, and an interrupt MAME took (a jump to $40-$60) is raised with
//! `request_interrupt` just before the core would dispatch it.
//!
//! Rust: the reference's `Divergence.history` holds disassembled text; the disassembler is not
//! transcribed, so here it holds the PCs of the last few instructions. Everything that
//! decides the result (the counts, the position, the problem lines) is transcribed exactly.

use std::collections::VecDeque;

use crate::core::{Bus, Sm83};
use crate::state::CpuState;
use crate::trace::{next_boundary, BoundaryKind};

pub const START_PC: u16 = 0x0100;
pub const VECTORS: [u16; 5] = [0x40, 0x48, 0x50, 0x58, 0x60];

/// One MAME instruction line (state before the instruction) and what it then read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub pc: u16,
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub ie: u8,
    pub if_: u8,
    /// MAME totalcycles, T-cycles.
    pub cycles: u64,
    /// I/O reads: (address, data).
    pub reads: Vec<(u16, u8)>,
}

/// Reads the replay answers from the log: I/O registers and IE (HRAM is RAM here).
pub fn is_io(address: u16) -> bool {
    address <= 0xFF7F || address == 0xFFFF
}

/// MAME instruction lines from the first one at `start_pc`, each with the I/O reads that
/// followed it. `SB` lines and MAME's own messages are skipped.
pub fn parse_trace(text: &str, start_pc: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut current: Option<Line> = None;
    let mut started = false;
    for raw in text.lines() {
        let fields: Vec<&str> = raw.split_whitespace().collect();
        if fields.len() == 4 && fields[0] == "R" {
            if started {
                if let Some(line) = current.as_mut() {
                    if let (Ok(address), Ok(data)) = (
                        u32::from_str_radix(fields[1], 16),
                        u32::from_str_radix(fields[2], 16),
                    ) {
                        let address = address as u16;
                        if is_io(address) {
                            line.reads.push((address, data as u8));
                        }
                    }
                }
            }
            continue;
        }
        if fields.len() != 13 {
            continue;
        }
        let mut values = Vec::with_capacity(13);
        let mut ok = true;
        for field in &fields[..12] {
            match u64::from_str_radix(field, 16) {
                Ok(v) => values.push(v),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        match fields[12].parse::<u64>() {
            Ok(v) => values.push(v),
            Err(_) => continue,
        }
        if !started {
            if values[0] != start_pc as u64 {
                continue;
            }
            started = true;
        }
        if let Some(line) = current.take() {
            out.push(line);
        }
        current = Some(Line {
            pc: values[0] as u16,
            a: values[1] as u8,
            f: values[2] as u8,
            b: values[3] as u8,
            c: values[4] as u8,
            d: values[5] as u8,
            e: values[6] as u8,
            h: values[7] as u8,
            l: values[8] as u8,
            sp: values[9] as u16,
            ie: values[10] as u8,
            if_: values[11] as u8,
            cycles: values[12],
            reads: Vec::new(),
        });
    }
    if let Some(line) = current {
        out.push(line);
    }
    out
}

/// The first place the core and MAME disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    /// Index of the MAME line the replay was executing when it diverged.
    pub position: usize,
    pub line: Line,
    pub problems: Vec<String>,
    /// PCs of the last few instructions, for context.
    pub history: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayResult {
    /// MAME lines compared without a difference.
    pub compared: u64,
    pub divergence: Option<Divergence>,
    /// Dispatches MAME took that the replay raised.
    pub interrupts: u64,
    /// Groups whose cycle delta was not compared (HALT).
    pub skipped_cycle_checks: u64,
    pub cycle_notes: Vec<String>,
    pub cycle_mismatches: u64,
}

impl ReplayResult {
    pub fn ok(&self) -> bool {
        self.divergence.is_none()
    }
}

/// ROM (MBC1-style bank register), RAM, and I/O reads answered from the MAME log.
pub struct ReplayHost {
    pub rom: Vec<u8>,
    pub banks: usize,
    pub bank: usize,
    pub memory: Vec<u8>,
    pub reads: VecDeque<(u16, u8)>,
    pub problems: Vec<String>,
}

impl ReplayHost {
    pub fn new(rom: &[u8]) -> Self {
        let mut padded = rom.to_vec();
        padded.resize(rom.len().max(0x8000), 0);
        let banks = (padded.len() / 0x4000).max(1);
        ReplayHost {
            rom: padded,
            banks,
            bank: 1,
            memory: vec![0; 0x10000],
            reads: VecDeque::new(),
            problems: Vec::new(),
        }
    }
}

impl Bus for ReplayHost {
    fn read(&mut self, address: u16, _cpu: &mut Sm83) -> u8 {
        let index = address as usize;
        if index < 0x4000 {
            return self.rom[index];
        }
        if index < 0x8000 {
            return self.rom[self.bank * 0x4000 + index - 0x4000];
        }
        if is_io(address) && address >= 0xFF00 {
            if self.reads.front().map(|r| r.0) != Some(address) {
                let expected = match self.reads.front() {
                    Some(r) => format!("{:04X}", r.0),
                    None => "nothing".into(),
                };
                self.problems
                    .push(format!("core read ${address:04X}, MAME logged {expected}"));
                return 0xFF;
            }
            return self.reads.pop_front().map(|r| r.1).unwrap_or(0xFF);
        }
        self.memory[index]
    }

    fn write(&mut self, address: u16, value: u8, cpu: &mut Sm83) {
        if address < 0x8000 {
            if (0x2000..0x4000).contains(&address) {
                let selected = if value & 0x1F != 0 {
                    (value & 0x1F) as usize
                } else {
                    1
                };
                self.bank = selected % self.banks;
            }
        } else if address == 0xFF0F {
            cpu.if_ = value & 0x1F; // the board's register, set from MAME's line anyway
        } else if !((0xFF00..=0xFF7F).contains(&address) || address == 0xFFFF) {
            self.memory[address as usize] = value; // I/O writes are compared by omission, not obeyed
        }
    }
}

/// Python's `f"{x:g}"` for the quarter-cycle values a cycle note prints.
fn format_g(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleMode {
    Strict,
    Report,
    Off,
}

fn register(state: &CpuState, name: &str) -> u32 {
    match name {
        "pc" => state.pc as u32,
        "a" => state.a as u32,
        "f" => state.f as u32,
        "b" => state.b as u32,
        "c" => state.c as u32,
        "d" => state.d as u32,
        "e" => state.e as u32,
        "h" => state.h as u32,
        "l" => state.l as u32,
        "sp" => state.sp as u32,
        _ => state.ie as u32,
    }
}

fn line_register(line: &Line, name: &str) -> u32 {
    match name {
        "pc" => line.pc as u32,
        "a" => line.a as u32,
        "f" => line.f as u32,
        "b" => line.b as u32,
        "c" => line.c as u32,
        "d" => line.d as u32,
        "e" => line.e as u32,
        "h" => line.h as u32,
        "l" => line.l as u32,
        "sp" => line.sp as u32,
        _ => line.ie as u32,
    }
}

const REGISTER_NAMES: [&str; 11] = ["pc", "a", "f", "b", "c", "d", "e", "h", "l", "sp", "ie"];

/// Run `rom` against MAME's lines; stop at the first divergence or the end/limit.
pub fn replay(
    rom: &[u8],
    lines: &[Line],
    limit: Option<u64>,
    cycles: CycleMode,
) -> Result<ReplayResult, String> {
    let mut host = ReplayHost::new(rom);
    let mut cpu = Sm83::new();
    let mut iterator = lines.iter();
    let mut current = iterator
        .next()
        .ok_or("the trace has no line at the cartridge entry point")?;
    cpu.restore_state(&CpuState {
        a: current.a,
        f: current.f & 0xF0,
        b: current.b,
        c: current.c,
        d: current.d,
        e: current.e,
        h: current.h,
        l: current.l,
        sp: current.sp,
        pc: current.pc,
        ie: current.ie,
        ..CpuState::default()
    });
    let mut history: VecDeque<u16> = VecDeque::with_capacity(6);
    let mut halted_cycles = false;
    let (mut compared, mut interrupts, mut skipped, mut mismatches) = (0u64, 0u64, 0u64, 0u64);
    let mut notes: Vec<String> = Vec::new();
    for (position, following) in iterator.enumerate() {
        if let Some(limit) = limit {
            if compared >= limit {
                break;
            }
        }
        host.reads = current.reads.iter().copied().collect();
        host.problems = Vec::new();
        cpu.if_ = current.if_ & 0x1F;
        let before = cpu.capture_state();
        if history.len() == 6 {
            history.pop_front();
        }
        history.push_back(before.pc);
        let mut used = cpu.step(&mut host) as u64;
        let after_halt = halted_cycles;
        halted_cycles = false;
        // An interrupt MAME took, or a HALT it was woken from, happens between two lines.
        for _ in 0..8 {
            let state = cpu.capture_state();
            let kind = next_boundary(&state);
            let took = VECTORS.contains(&following.pc) && state.pc != following.pc;
            if kind == BoundaryKind::HaltIdle {
                halted_cycles = true;
                if !took {
                    // woken without a dispatch (IME off): MAME's next line is the byte after
                    // the HALT, about to execute. The core's wake step fetches and runs it, so
                    // leave the core halted; the next group wakes it (its IF is MAME's).
                    break;
                }
                let enabled = state.ie & 0x1F;
                // the source MAME served, plus the higher ones left pending after it
                cpu.if_ |= (following.if_ & enabled) | 1 << ((following.pc - 0x40) / 8);
            } else if took && kind == BoundaryKind::Instruction && state.ime {
                cpu.request_interrupt(((following.pc - 0x40) / 8) as u8, false, false);
                interrupts += 1;
            } else if kind != BoundaryKind::Dispatch {
                break;
            }
            used += cpu.step(&mut host) as u64;
        }
        let state = cpu.capture_state();
        let mut problems = host.problems.clone();
        if let Some(first) = host.reads.front() {
            problems.push(format!(
                "MAME logged {} more I/O read(s), first ${:04X}",
                host.reads.len(),
                first.0
            ));
        }
        for name in REGISTER_NAMES {
            let mut want = line_register(following, name);
            if name == "f" {
                want &= 0xF0;
            }
            let got = register(&state, name);
            if got != want {
                problems.push(format!("{} = {got:X}, MAME {want:X}", name.to_uppercase()));
            }
        }
        if halted_cycles || after_halt {
            skipped += 1;
        } else if cycles != CycleMode::Off {
            let difference = following.cycles as i64 - current.cycles as i64;
            let (delta, remainder) = (difference.div_euclid(4), difference.rem_euclid(4));
            if remainder != 0 || delta != used as i64 {
                let note = format!(
                    "{used} M-cycles, MAME {}",
                    format_g(difference as f64 / 4.0)
                );
                if cycles == CycleMode::Strict {
                    problems.push(note);
                } else {
                    mismatches += 1;
                    if notes.len() < 20 {
                        notes.push(format!(
                            "line {position} {:04X}: {note}",
                            history.back().copied().unwrap_or(0)
                        ));
                    }
                }
            }
        }
        if !problems.is_empty() {
            return Ok(ReplayResult {
                compared,
                divergence: Some(Divergence {
                    position,
                    line: current.clone(),
                    problems,
                    history: history.iter().copied().collect(),
                }),
                interrupts,
                skipped_cycle_checks: skipped,
                cycle_notes: notes,
                cycle_mismatches: mismatches,
            });
        }
        compared += 1;
        current = following;
    }
    Ok(ReplayResult {
        compared,
        divergence: None,
        interrupts,
        skipped_cycle_checks: skipped,
        cycle_notes: notes,
        cycle_mismatches: mismatches,
    })
}
