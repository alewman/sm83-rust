//! What the board tells the core about a `STOP` (`stop.py`; docs/timing.md "STOP").
//!
//! The core cannot decide STOP alone: the outcome depends on the joypad lines, KEY1 and the
//! interrupt state (SameBoy `stop()`, SonoSooS "NOP and STOP"). On `$10` the core calls
//! [`crate::Bus::stop`] with `pending` and carries out the [`StopResult`] the board returns.
//! A board that switches CPU speed does it inside the hook and then returns
//! [`StopAction::Halt`].

/// The state STOP leaves the core in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StopAction {
    /// Enter stop mode: `stopped`, one idle cycle per step until the board wakes it.
    Stop,
    /// Behave as HALT: `halted`, waking on a pending interrupt.
    Halt,
    /// Behave as a NOP: execution continues.
    Nop,
}

/// The board's answer to a STOP: the action, and whether the second byte is consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StopResult {
    pub action: StopAction,
    pub consume_operand: bool,
}

impl StopResult {
    /// `StopResult(action)`: the operand is consumed (the reference's default).
    pub const fn new(action: StopAction) -> Self {
        StopResult {
            action,
            consume_operand: true,
        }
    }

    pub const fn with_operand(action: StopAction, consume_operand: bool) -> Self {
        StopResult {
            action,
            consume_operand,
        }
    }
}
