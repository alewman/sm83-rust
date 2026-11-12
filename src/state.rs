//! Immutable values for capturing and restoring SM83 processor state (`state.py`).
//!
//! Rust: the field types enforce the reference's ranges (`u8`, `u16`,
//! `bool`); the one check they cannot express, F's missing low nibble, is
//! [`CpuState::validate`], which `Sm83::restore_state` asserts.

/// Complete CPU-owned state at an instruction boundary.
///
/// Registers, IE and IF (both live in the core), IME and the delayed-EI flag, and the
/// halted / stopped / locked / HALT-bug conditions. It excludes host memory, devices,
/// scheduling and counters, so restoring it restores the processor, not a machine. `f`
/// keeps only bits 7-4 (the low nibble does not exist).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CpuState {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
    pub ie: u8,
    pub if_: u8,
    pub ime: bool,
    pub ei_pending: bool,
    pub halted: bool,
    pub halt_bug: bool,
    pub halt_dispatch: bool,
    pub stopped: bool,
    pub locked: bool,
    pub stop_if: u8,
}

impl CpuState {
    /// The reference's `__post_init__` check that the types cannot make.
    pub fn validate(&self) -> Result<(), String> {
        if self.f & 0x0F != 0 {
            return Err("f bits 3..0 do not exist and must be 0".into());
        }
        Ok(())
    }
}
