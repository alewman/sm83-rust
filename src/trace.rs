//! Deterministic processor-boundary traces in the reference's version 1 JSON Lines schema
//! (`trace.py`, with the parts of `debug.py` and `disasm.py` a writer needs;
//! sm83-python docs/trace-schema.md).
//!
//! A record carries the boundary kind, the M-cycles, the instruction's address and bytes (no
//! mnemonic: the schema tells a producer in another language to omit it, and the reference's
//! reader disassembles the bytes), the complete state before and after, and every M-cycle
//! when recorded. Only writing is transcribed; comparing is the reference's job
//! (`first_trace_divergence` over `read_trace`).
//!
//! Rust: the reference's `DebugSession` records accesses by swapping the CPU's bus callables
//! for wrappers; here [`Tracked`] wraps the host's [`Bus`] and [`step_record`] is
//! `DebugSession.step`.

use serde_json::{json, Map, Value};

use crate::core::{Bus, Sm83, IE_ADDRESS, INTERRUPT_BITS};
use crate::opcodes::{CB_OPCODES, OPCODES};
use crate::state::CpuState;
use crate::stop::StopResult;

pub const TRACE_SCHEMA_VERSION: u32 = 1;

/// Kind of work one `step()` call starts with (`debug.BoundaryKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoundaryKind {
    Instruction,
    /// An interrupt is taken: five M-cycles.
    Dispatch,
    /// Halted: one idle cycle, and a wake-up may dispatch in the same step.
    HaltIdle,
    /// In stop mode: one idle cycle.
    StopIdle,
    /// An undefined opcode locked the CPU: one idle cycle, forever.
    LockedIdle,
}

impl BoundaryKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            BoundaryKind::Instruction => "instruction",
            BoundaryKind::Dispatch => "dispatch",
            BoundaryKind::HaltIdle => "halt_idle",
            BoundaryKind::StopIdle => "stop_idle",
            BoundaryKind::LockedIdle => "locked_idle",
        }
    }
}

/// What the next `step()` starts with, decided exactly as `Sm83::step()` decides it.
pub fn next_boundary(state: &CpuState) -> BoundaryKind {
    if state.locked {
        return BoundaryKind::LockedIdle;
    }
    if state.stopped {
        return BoundaryKind::StopIdle;
    }
    if state.halted {
        return BoundaryKind::HaltIdle;
    }
    if state.ime && state.ie & state.if_ & INTERRUPT_BITS != 0 {
        return BoundaryKind::Dispatch;
    }
    BoundaryKind::Instruction
}

/// The bytes of the instruction at `address`, as the reference's disassembler takes them:
/// two for a CB opcode, one for a hole, else the opcode table's length.
pub fn instruction_bytes(peek: &dyn Fn(u16) -> u8, address: u16) -> Vec<u8> {
    let opcode = peek(address);
    if opcode == 0xCB {
        let second = peek(address.wrapping_add(1));
        let _ = CB_OPCODES[second as usize];
        return vec![opcode, second];
    }
    let entry = OPCODES[opcode as usize];
    let length = if entry.category == "undefined" {
        1
    } else {
        entry.length
    };
    (0..length as u16)
        .map(|i| peek(address.wrapping_add(i)))
        .collect()
}

/// One M-cycle as the CPU drove the bus (`debug.Access`): kind, address, value (none for an
/// internal cycle), IDU direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read(u16, u8, i8),
    Write(u16, u8, i8),
    Internal(u16, i8),
}

/// Immutable before/after evidence for one processor boundary (`debug.StepRecord`).
#[derive(Clone, Debug)]
pub struct StepRecord {
    pub sequence: u64,
    pub kind: BoundaryKind,
    pub before: CpuState,
    pub after: CpuState,
    pub m_cycles: u32,
    /// The instruction's address and bytes, for an instruction boundary.
    pub instruction: Option<(u16, Vec<u8>)>,
    pub accesses: Option<Vec<Access>>,
}

fn state_value(state: &CpuState) -> Value {
    json!({
        "a": state.a,
        "f": state.f,
        "b": state.b,
        "c": state.c,
        "d": state.d,
        "e": state.e,
        "h": state.h,
        "l": state.l,
        "sp": state.sp,
        "pc": state.pc,
        "ie": state.ie,
        "if_": state.if_,
        "ime": state.ime,
        "ei_pending": state.ei_pending,
        "halted": state.halted,
        "halt_bug": state.halt_bug,
        "halt_dispatch": state.halt_dispatch,
        "stopped": state.stopped,
        "locked": state.locked,
        "stop_if": state.stop_if,
    })
}

impl StepRecord {
    /// The versioned JSON representation (`step_record_to_dict`, without mnemonic).
    pub fn to_value(&self) -> Value {
        let mut record = Map::new();
        record.insert("version".into(), json!(TRACE_SCHEMA_VERSION));
        record.insert("sequence".into(), json!(self.sequence));
        record.insert("kind".into(), json!(self.kind.as_str()));
        record.insert("m_cycles".into(), json!(self.m_cycles));
        let instruction = match &self.instruction {
            None => Value::Null,
            Some((address, data)) => {
                let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
                json!({"address": address, "data": hex})
            }
        };
        record.insert("instruction".into(), instruction);
        record.insert("before".into(), state_value(&self.before));
        record.insert("after".into(), state_value(&self.after));
        if let Some(accesses) = &self.accesses {
            let list: Vec<Value> = accesses
                .iter()
                .map(|access| match *access {
                    Access::Read(address, value, idu) => json!(["read", address, value, idu]),
                    Access::Write(address, value, idu) => json!(["write", address, value, idu]),
                    Access::Internal(address, idu) => json!(["internal", address, null, idu]),
                })
                .collect();
            record.insert("accesses".into(), Value::Array(list));
        }
        Value::Object(record)
    }

    /// One JSON Lines line, without the newline.
    pub fn to_line(&self) -> String {
        self.to_value().to_string()
    }
}

/// A host's bus with every M-cycle recorded (`DebugSession(track_accesses=True)`).
pub struct Tracked<'a, B: Bus + ?Sized> {
    pub inner: &'a mut B,
    pub log: Vec<Access>,
}

impl<'a, B: Bus + ?Sized> Tracked<'a, B> {
    pub fn new(inner: &'a mut B) -> Self {
        Tracked {
            inner,
            log: Vec::new(),
        }
    }
}

impl<B: Bus + ?Sized> Bus for Tracked<'_, B> {
    fn read(&mut self, address: u16, cpu: &mut Sm83) -> u8 {
        let value = self.inner.read(address, cpu);
        // the core owns $FFFF and uses IE, whatever the host returned
        let seen = if address == IE_ADDRESS { cpu.ie } else { value };
        self.log.push(Access::Read(address, seen, cpu.idu));
        value
    }

    fn write(&mut self, address: u16, value: u8, cpu: &mut Sm83) {
        self.log.push(Access::Write(address, value, cpu.idu));
        self.inner.write(address, value, cpu);
    }

    fn idle(&mut self, address: u16, cpu: &mut Sm83) {
        self.log.push(Access::Internal(address, cpu.idu));
        self.inner.idle(address, cpu);
    }

    fn stop(&mut self, pending: bool, cpu: &mut Sm83) -> StopResult {
        self.inner.stop(pending, cpu)
    }
}

/// Advance exactly one boundary and record it (`DebugSession.step`). `peek` must be
/// side-effect-free (the host's memory, not its bus); `None` records no instruction bytes.
/// `track` records every M-cycle.
pub fn step_record<B: Bus + ?Sized>(
    cpu: &mut Sm83,
    bus: &mut B,
    peek: Option<&dyn Fn(u16) -> u8>,
    sequence: u64,
    track: bool,
) -> StepRecord {
    let before = cpu.capture_state();
    let kind = next_boundary(&before);
    let instruction = match (kind, peek) {
        (BoundaryKind::Instruction, Some(peek)) => {
            Some((before.pc, instruction_bytes(peek, before.pc)))
        }
        _ => None,
    };
    let (m_cycles, accesses) = if track {
        let mut tracked = Tracked::new(bus);
        let m_cycles = cpu.step(&mut tracked);
        (m_cycles, Some(tracked.log))
    } else {
        (cpu.step(bus), None)
    };
    StepRecord {
        sequence,
        kind,
        before,
        after: cpu.capture_state(),
        m_cycles,
        instruction,
        accesses,
    }
}
