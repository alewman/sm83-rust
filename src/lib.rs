//! A Rust Sharp SM83 (Game Boy CPU) core transcribed from sm83-python at `95f2b8b` and
//! proven equivalent to it at every M-cycle: every bus access in order (address, value,
//! direction, the IDU), boundary kinds, and all of `CpuState`.
//!
//! Each module is the transcription of one `sm83_python/*.py` module, with the same handler
//! names, the same explicit dispatch and the hardware comments carried over. Where Rust
//! forced a difference, the module header says so; the one that matters is the bus
//! ([`Bus`]: the host is passed to [`Sm83::step`], and each call also gets `&mut Sm83`).

mod alu;
mod control;
pub mod core;
mod cpu;
pub mod dispatch;
pub mod flags;
pub mod harness;
mod loads;
pub mod opcodes;
pub mod replay;
mod rotate;
pub mod state;
pub mod stop;
pub mod trace;

pub use crate::core::{pack, Bus, Sm83, IE_ADDRESS, INTERRUPT_BITS};
pub use crate::flags::{FLAG_C, FLAG_H, FLAG_N, FLAG_Z};
pub use crate::state::CpuState;
pub use crate::stop::{StopAction, StopResult};
