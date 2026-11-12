//! Rotates, shifts, SWAP and single-bit operations: the A-register rotates and the CB page
//! (`_rotate.py`).
//!
//! `$CB` is an instruction whose second cycle fetches the CB opcode without an interrupt
//! check, so CB instructions are uninterruptible two-byte opcodes (SonoSooS "$CB prefix";
//! U10). Every CB operation takes the value in and returns the new value, setting its flags.
//!
//! Rust: values are widened to `u32` where the reference's integers would exceed a byte, and
//! the result masked, as `_rotate_a` and `_shifted` mask it.

use crate::alu::zero_flag;
use crate::core::{Bus, Sm83};
use crate::flags::{FLAG_C, FLAG_H, FLAG_Z};

impl Sm83 {
    // ----- the four rotates of A: Z is always cleared ---------------------

    fn rotate_a(&mut self, result: u32, carry_out: u8) {
        self.a = result as u8;
        self.f = if carry_out != 0 { FLAG_C } else { 0 };
    }

    /// RLCA -- rotate A left; bit 7 goes to C and bit 0 (GBCTR RLCA).
    pub(crate) fn op_rlca<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        let a = self.a as u32;
        self.rotate_a((a << 1) | (a >> 7), self.a & 0x80);
    }

    /// RRCA -- rotate A right; bit 0 goes to C and bit 7 (GBCTR RRCA).
    pub(crate) fn op_rrca<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        let a = self.a as u32;
        self.rotate_a((a >> 1) | (a << 7), self.a & 1);
    }

    /// RLA -- rotate A left through C (GBCTR RLA).
    pub(crate) fn op_rla<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        let a = self.a as u32;
        let carry = if self.f & FLAG_C != 0 { 1 } else { 0 };
        self.rotate_a((a << 1) | carry, self.a & 0x80);
    }

    /// RRA -- rotate A right through C (GBCTR RRA).
    pub(crate) fn op_rra<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        let a = self.a as u32;
        let carry = if self.f & FLAG_C != 0 { 0x80 } else { 0 };
        self.rotate_a((a >> 1) | carry, self.a & 1);
    }

    // ----- the CB page ----------------------------------------------------

    /// Flags of a CB rotate/shift: Z from the result, N = H = 0, C the bit shifted out.
    fn shifted(&mut self, result: u32, carry_out: u32) -> u8 {
        let result = result & 0xFF;
        self.f = zero_flag(result) | if carry_out != 0 { FLAG_C } else { 0 };
        result as u8
    }

    /// RLC r/RLC (HL) -- rotate left; bit 7 goes to C and bit 0 (GBCTR RLC_r, RLC_hl).
    fn cb_rlc(&mut self, value: u32) -> u8 {
        self.shifted((value << 1) | (value >> 7), value & 0x80)
    }

    /// RRC r/RRC (HL) -- rotate right; bit 0 goes to C and bit 7 (GBCTR RRC_r, RRC_hl).
    fn cb_rrc(&mut self, value: u32) -> u8 {
        self.shifted((value >> 1) | (value << 7), value & 1)
    }

    /// RL r/RL (HL) -- rotate left through C (GBCTR RL_r, RL_hl).
    fn cb_rl(&mut self, value: u32) -> u8 {
        let carry = if self.f & FLAG_C != 0 { 1 } else { 0 };
        self.shifted((value << 1) | carry, value & 0x80)
    }

    /// RR r/RR (HL) -- rotate right through C (GBCTR RR_r, RR_hl).
    fn cb_rr(&mut self, value: u32) -> u8 {
        let carry = if self.f & FLAG_C != 0 { 0x80 } else { 0 };
        self.shifted((value >> 1) | carry, value & 1)
    }

    /// SLA r/SLA (HL) -- shift left, bit 0 becomes 0 (GBCTR SLA_r, SLA_hl).
    fn cb_sla(&mut self, value: u32) -> u8 {
        self.shifted(value << 1, value & 0x80)
    }

    /// SRA r/SRA (HL) -- arithmetic shift right, bit 7 is kept (GBCTR SRA_r, SRA_hl).
    fn cb_sra(&mut self, value: u32) -> u8 {
        self.shifted((value >> 1) | (value & 0x80), value & 1)
    }

    /// SWAP r/SWAP (HL) -- exchange the nibbles; C is cleared (GBCTR SWAP_r, SWAP_hl).
    fn cb_swap(&mut self, value: u32) -> u8 {
        self.shifted((value << 4) | (value >> 4), 0)
    }

    /// SRL r/SRL (HL) -- logical shift right, bit 7 becomes 0 (GBCTR SRL_r, SRL_hl).
    fn cb_srl(&mut self, value: u32) -> u8 {
        self.shifted(value >> 1, value & 1)
    }

    /// BIT b,r/BIT b,(HL) -- Z = bit b is clear; H set; C kept (GBCTR BIT_b_r, BIT_b_hl).
    fn cb_bit(&mut self, bit: u8, value: u8) {
        self.f = (self.f & FLAG_C) | FLAG_H | if (value >> bit) & 1 != 0 { 0 } else { FLAG_Z };
    }

    /// RES b,r/RES b,(HL) -- clear bit b; no flags (GBCTR RES_b_r, RES_b_hl).
    fn cb_res(&mut self, bit: u8, value: u8) -> u8 {
        value & !(1 << bit)
    }

    /// SET b,r/SET b,(HL) -- set bit b; no flags (GBCTR SET_b_r, SET_b_hl).
    fn cb_set(&mut self, bit: u8, value: u8) -> u8 {
        value | (1 << bit)
    }

    /// CB -- prefix: the second byte selects a rotate, shift, BIT, RES or SET (GBCTR CB).
    pub(crate) fn op_cb<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let cb = self.read_pc(bus); // fetched without an interrupt check (U10)
        let (operation, index) = ((cb >> 3) & 7, cb & 7);
        // (HL): BIT reads once; everything else reads then writes
        let value = if index == 6 {
            self.read(bus, self.hl(), 0)
        } else {
            self.get_r8(index)
        };
        let group = cb >> 6;
        if group == 1 {
            self.cb_bit(operation, value);
            return;
        }
        let result = if group == 0 {
            let value = value as u32;
            match operation {
                0 => self.cb_rlc(value),
                1 => self.cb_rrc(value),
                2 => self.cb_rl(value),
                3 => self.cb_rr(value),
                4 => self.cb_sla(value),
                5 => self.cb_sra(value),
                6 => self.cb_swap(value),
                _ => self.cb_srl(value),
            }
        } else if group == 2 {
            self.cb_res(operation, value)
        } else {
            self.cb_set(operation, value)
        };
        if index == 6 {
            self.write(bus, self.hl(), result, 0);
        } else {
            self.set_r8(index, result);
        }
    }
}
