//! Control flow and processor state: JP, JR, CALL, RET, RST, DI, EI, NOP, HALT, STOP
//! (`_control.py`).
//!
//! The 11 unused opcodes also live here: they lock the CPU, which never fetches again (U3).
//!
//! Rust: the reference calls its `stop_hook` (or uses the default result when there is
//! none); here the hook is [`Bus::stop`], whose default is that result.

use crate::core::{Bus, Sm83, INTERRUPT_BITS};
use crate::loads::signed;
use crate::stop::StopAction;

impl Sm83 {
    /// NOP -- no operation; one M-cycle, the fetch (GBCTR NOP).
    pub(crate) fn op_nop<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {}

    // ----- jumps ----------------------------------------------------------

    fn jump_relative<B: Bus + ?Sized>(&mut self, bus: &mut B) {
        let offset = self.read_pc(bus);
        // The ALU adds the offset to PCL while the IDU adjusts PCH for the carry (GBCTR JR "W
        // <- adj PCH"). The address bus carries PCH in its high byte and zero below it:
        // Gekkio's die model drives $2400 for a JR whose operand fetch left PC = $2402 (rung
        // 5, all 14 JR cases at 10 low addresses, forward and backward).
        self.idle(bus, self.pc & 0xFF00, 0);
        self.pc = self.pc.wrapping_add(signed(offset) as u16);
    }

    /// JR e -- relative jump by a signed byte (GBCTR JR).
    pub(crate) fn op_jr<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        self.jump_relative(bus);
    }

    /// JR cc,e -- relative jump if the condition holds (GBCTR JR_cc).
    pub(crate) fn op_jr_cc<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        if self.condition((op >> 3) & 3) {
            self.jump_relative(bus);
        } else {
            self.read_pc(bus); // the operand is still fetched
        }
    }

    fn jump_absolute<B: Bus + ?Sized>(&mut self, bus: &mut B) {
        let address = self.read_pc_word(bus);
        self.idle(bus, 0x0000, 0); // PC <- WZ; nothing is driven (GBCTR: $0000)
        self.pc = address;
    }

    /// JP nn -- jump to an absolute address (GBCTR JP).
    pub(crate) fn op_jp<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        self.jump_absolute(bus);
    }

    /// JP cc,nn -- jump if the condition holds (GBCTR JP_cc).
    pub(crate) fn op_jp_cc<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        if self.condition((op >> 3) & 3) {
            self.jump_absolute(bus);
        } else {
            self.read_pc_word(bus);
        }
    }

    /// JP HL -- jump to HL; the next fetch is the instruction's own last cycle (GBCTR JP_hl).
    pub(crate) fn op_jp_hl<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        self.pc = self.hl();
    }

    // ----- calls and returns ----------------------------------------------

    fn call<B: Bus + ?Sized>(&mut self, bus: &mut B) {
        let address = self.read_pc_word(bus);
        self.push_word(bus, self.pc);
        self.pc = address;
    }

    /// CALL nn -- push the return address and jump (GBCTR CALL).
    pub(crate) fn op_call<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        self.call(bus);
    }

    /// CALL cc,nn -- call if the condition holds (GBCTR CALL_cc).
    pub(crate) fn op_call_cc<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        if self.condition((op >> 3) & 3) {
            self.call(bus);
        } else {
            self.read_pc_word(bus);
        }
    }

    fn return_<B: Bus + ?Sized>(&mut self, bus: &mut B) {
        let address = self.pop_word(bus);
        self.idle(bus, 0x0000, 0); // PC <- WZ; nothing is driven (GBCTR: $0000)
        self.pc = address;
    }

    /// RET -- pop the return address into PC (GBCTR RET).
    pub(crate) fn op_ret<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        self.return_(bus);
    }

    /// RETI -- RET, and IME is set at once, no delay (GBCTR RETI; Mooneye reti_intr_timing).
    pub(crate) fn op_reti<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        self.return_(bus);
        self.ime = true;
    }

    /// RET cc -- return if the condition holds (GBCTR RET_cc).
    pub(crate) fn op_ret_cc<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        self.idle(bus, 0x0000, 0); // the condition check is a cycle of its own
        if self.condition((op >> 3) & 3) {
            self.return_(bus);
        }
    }

    /// RST n -- push PC and jump to $00, $08 .. $38 (GBCTR RST).
    pub(crate) fn op_rst<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        self.push_word(bus, self.pc);
        self.pc = (op & 0x38) as u16;
    }

    // ----- interrupt master enable ----------------------------------------

    /// DI -- disable interrupts at once, cancelling a pending EI (GBCTR DI; rapid_di_ei).
    pub(crate) fn op_di<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        self.ime = false;
        self.ei_pending = false;
    }

    /// EI -- enable interrupts after the next instruction (GBCTR EI; Mooneye ei_sequence).
    pub(crate) fn op_ei<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        self.ei_pending = true;
    }

    // ----- HALT, STOP and the holes ---------------------------------------

    /// HALT -- stop executing until an interrupt is pending (GBCTR HALT; U4-U6).
    pub(crate) fn op_halt<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        // The decision uses the interrupt state sampled before HALT's own fetch, as dispatch
        // does: an interrupt raised during that fetch is not seen (die model, rung 5).
        if self.pending_at_fetch {
            if self.ime {
                self.halt_dispatch = true; // no halt: the dispatch pushes the HALT itself (U6)
            } else {
                // the HALT bug (U4): no halt, and the next fetch does not increment PC. The die
                // makes no extra cycle: the byte after the HALT is fetched, and fetched again
                self.halt_bug = true;
            }
            return;
        }
        // HALT's own M-cycle fetches the next byte without incrementing PC (SonoSooS: IR =
        // [PC]); the die shows it as an opcode fetch at the next address before its clock
        // stops, and the byte is fetched again on wake-up. SameBoy halt() reads the same way.
        self.read(bus, self.pc, 0);
        self.halted = true;
    }

    /// STOP -- the board decides the outcome through the stop hook (timing.md "STOP"; U11).
    pub(crate) fn op_stop<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let pending = self.interrupt_pending();
        // the default is a 2-byte STOP that stops until IF changes
        let result = bus.stop(pending, self);
        if result.consume_operand {
            self.read_pc(bus);
        }
        match result.action {
            StopAction::Stop => {
                self.stopped = true;
                self.stop_if = self.if_ & INTERRUPT_BITS;
            }
            StopAction::Halt => self.halted = true,
            StopAction::Nop => {}
        }
    }

    /// ILLEGAL -- the 11 unused opcodes lock the CPU; no further fetch happens (U3).
    pub(crate) fn op_undefined<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        self.locked = true;
    }
}
