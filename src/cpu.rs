//! Public SM83 CPU API (`cpu.py`).
//!
//! The host owns memory and every device. It implements [`crate::Bus`] (`read`, `write`,
//! and optionally `idle` and `stop`), calls [`Sm83::step`] with it, and advances its devices
//! **inside** the bus calls: every M-cycle is one call, in chip order, internal cycles
//! included, so the host never adds `step()`'s return value on top.
//!
//! ```
//! use sm83::{Bus, Sm83};
//!
//! struct Ram(Vec<u8>);
//! impl Bus for Ram {
//!     fn read(&mut self, address: u16, _: &mut Sm83) -> u8 { self.0[address as usize] }
//!     fn write(&mut self, address: u16, value: u8, _: &mut Sm83) { self.0[address as usize] = value; }
//! }
//!
//! let mut ram = Ram(vec![0; 0x10000]);
//! let mut cpu = Sm83::new();
//! assert_eq!(cpu.step(&mut ram), 1); // NOP at $0000
//! ```
//!
//! Rust: the reference's constructor takes the bus callables and checks they are callable;
//! here the bus is a trait the compiler checks, passed to `step`, so `new()` takes nothing.

use crate::core::Sm83;
use crate::state::CpuState;

impl Sm83 {
    /// A CPU with zeroed state.
    pub fn new() -> Self {
        let mut cpu = Sm83::default();
        cpu.restore_state(&CpuState::default());
        cpu
    }

    /// Reset the processor: PC = 0, IME = 0, IE = 0, no pending EI, running. The other
    /// registers keep their values (power-on values are undocumented, docs/start-here.md).
    pub fn reset(&mut self) {
        self.pc = 0;
        self.ie = 0;
        self.ime = false;
        self.ei_pending = false;
        self.halted = false;
        self.halt_bug = false;
        self.halt_dispatch = false;
        self.stopped = false;
        self.locked = false;
    }

    /// The complete CPU-owned state as an immutable value.
    pub fn capture_state(&self) -> CpuState {
        CpuState {
            a: self.a,
            f: self.f,
            b: self.b,
            c: self.c,
            d: self.d,
            e: self.e,
            h: self.h,
            l: self.l,
            sp: self.sp,
            pc: self.pc,
            ie: self.ie,
            if_: self.if_,
            ime: self.ime,
            ei_pending: self.ei_pending,
            halted: self.halted,
            halt_bug: self.halt_bug,
            halt_dispatch: self.halt_dispatch,
            stopped: self.stopped,
            locked: self.locked,
            stop_if: self.stop_if,
        }
    }

    /// Replace the CPU-owned state with a captured one. Panics on a state the reference's
    /// `CPUState` would refuse to construct (F's low nibble set).
    pub fn restore_state(&mut self, state: &CpuState) {
        if let Err(problem) = state.validate() {
            panic!("invalid CPU state: {problem}");
        }
        self.a = state.a;
        self.f = state.f;
        self.b = state.b;
        self.c = state.c;
        self.d = state.d;
        self.e = state.e;
        self.h = state.h;
        self.l = state.l;
        self.sp = state.sp;
        self.pc = state.pc;
        self.ie = state.ie;
        self.if_ = state.if_;
        self.ime = state.ime;
        self.ei_pending = state.ei_pending;
        self.halted = state.halted;
        self.halt_bug = state.halt_bug;
        self.halt_dispatch = state.halt_dispatch;
        self.stopped = state.stopped;
        self.locked = state.locked;
        self.stop_if = state.stop_if;
    }

    /// M-cycles made since construction (every bus call counted once).
    pub fn cycles(&self) -> u64 {
        self.cycles
    }
}
