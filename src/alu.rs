//! Arithmetic and logic: the eight A-register ALU operations, INC/DEC, 16-bit adds, DAA
//! (`_alu.py`).
//!
//! Flags are Z N H C in F bits 7-4 (Pan Docs CPU_Instruction_Set.md; GBCTR's per-instruction
//! `flags` fields). H is the carry out of bit 3 (bit 11 for 16-bit adds); C the carry out of
//! the top bit; for subtraction they are borrows.
//!
//! Rust: `self.a &= operand` in Python reads A before the operand's bus cycle; the handlers
//! below read it first too.

use crate::core::{Bus, Sm83};
use crate::flags::{FLAG_C, FLAG_H, FLAG_N, FLAG_Z};
use crate::loads::signed;

pub fn zero_flag(result: u32) -> u8 {
    if result & 0xFF == 0 {
        FLAG_Z
    } else {
        0
    }
}

impl Sm83 {
    /// The source of an A-register ALU opcode: n, (HL) or a register by its encoding.
    fn alu_operand<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) -> u8 {
        if op & 0xC0 == 0xC0 {
            // $C6..$FE: immediate operand
            return self.read_pc(bus);
        }
        if op & 7 == 6 {
            // (HL)
            return self.read(bus, self.hl(), 0);
        }
        self.get_r8(op & 7)
    }

    fn add(&mut self, value: u8, carry: u8) {
        let total = self.a as u32 + value as u32 + carry as u32;
        self.f = zero_flag(total)
            | if (self.a & 0xF) as u32 + (value & 0xF) as u32 + carry as u32 > 0xF {
                FLAG_H
            } else {
                0
            }
            | if total > 0xFF { FLAG_C } else { 0 };
        self.a = total as u8;
    }

    /// A - value - carry with SUB/SBC/CP flags; returns the result without storing it.
    fn subtract(&mut self, value: u8, carry: u8) -> u8 {
        let total = self.a as i32 - value as i32 - carry as i32;
        self.f = FLAG_N
            | zero_flag(total as u32)
            | if ((self.a & 0xF) as i32) < (value & 0xF) as i32 + carry as i32 {
                FLAG_H
            } else {
                0
            }
            | if total < 0 { FLAG_C } else { 0 };
        total as u8
    }

    /// ADD A,r/ADD A,(HL)/ADD A,n -- A = A + operand (GBCTR ADD_r, ADD_hl, ADD_n).
    pub(crate) fn op_add<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let value = self.alu_operand(bus, op);
        self.add(value, 0);
    }

    /// ADC A,r/ADC A,(HL)/ADC A,n -- A = A + operand + carry (GBCTR ADC_r, ADC_hl, ADC_n).
    pub(crate) fn op_adc<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let value = self.alu_operand(bus, op);
        self.add(value, if self.f & FLAG_C != 0 { 1 } else { 0 });
    }

    /// SUB r/SUB (HL)/SUB n -- A = A - operand (GBCTR SUB_r, SUB_hl, SUB_n).
    pub(crate) fn op_sub<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let value = self.alu_operand(bus, op);
        self.a = self.subtract(value, 0);
    }

    /// SBC A,r/SBC A,(HL)/SBC A,n -- A = A - operand - carry (GBCTR SBC_r, SBC_hl, SBC_n).
    pub(crate) fn op_sbc<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let value = self.alu_operand(bus, op);
        self.a = self.subtract(value, if self.f & FLAG_C != 0 { 1 } else { 0 });
    }

    /// AND r/AND (HL)/AND n -- A = A & operand; H is always set (GBCTR AND_r, AND_hl, AND_n).
    pub(crate) fn op_and<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let a = self.a;
        self.a = a & self.alu_operand(bus, op);
        self.f = zero_flag(self.a as u32) | FLAG_H;
    }

    /// XOR r/XOR (HL)/XOR n -- A = A ^ operand (GBCTR XOR_r, XOR_hl, XOR_n).
    pub(crate) fn op_xor<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let a = self.a;
        self.a = a ^ self.alu_operand(bus, op);
        self.f = zero_flag(self.a as u32);
    }

    /// OR r/OR (HL)/OR n -- A = A | operand (GBCTR OR_r, OR_hl, OR_n).
    pub(crate) fn op_or<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let a = self.a;
        self.a = a | self.alu_operand(bus, op);
        self.f = zero_flag(self.a as u32);
    }

    /// CP r/CP (HL)/CP n -- compare: SUB's flags, A unchanged (GBCTR CP_r, CP_hl, CP_n).
    pub(crate) fn op_cp<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let value = self.alu_operand(bus, op);
        self.subtract(value, 0);
    }

    fn inc8(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        // C is untouched; N = 0; H = carry out of bit 3
        self.f = (self.f & FLAG_C)
            | zero_flag(result as u32)
            | if value & 0xF == 0xF { FLAG_H } else { 0 };
        result
    }

    fn dec8(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        // C is untouched; N = 1; H = borrow from bit 4
        self.f = (self.f & FLAG_C)
            | FLAG_N
            | zero_flag(result as u32)
            | if value & 0xF == 0 { FLAG_H } else { 0 };
        result
    }

    /// INC r -- increment a register (GBCTR INC_r).
    pub(crate) fn op_inc_r<B: Bus + ?Sized>(&mut self, _bus: &mut B, op: u8) {
        let index = (op >> 3) & 7;
        let value = self.inc8(self.get_r8(index));
        self.set_r8(index, value);
    }

    /// DEC r -- decrement a register (GBCTR DEC_r).
    pub(crate) fn op_dec_r<B: Bus + ?Sized>(&mut self, _bus: &mut B, op: u8) {
        let index = (op >> 3) & 7;
        let value = self.dec8(self.get_r8(index));
        self.set_r8(index, value);
    }

    /// INC (HL) -- read-modify-write at HL (GBCTR INC_hl).
    pub(crate) fn op_inc_hl<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let value = self.read(bus, self.hl(), 0);
        let address = self.hl();
        let result = self.inc8(value);
        self.write(bus, address, result, 0);
    }

    /// DEC (HL) -- read-modify-write at HL (GBCTR DEC_hl).
    pub(crate) fn op_dec_hl<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let value = self.read(bus, self.hl(), 0);
        let address = self.hl();
        let result = self.dec8(value);
        self.write(bus, address, result, 0);
    }

    /// INC rr -- 16-bit increment through the IDU, no flags (GBCTR INC_rr).
    pub(crate) fn op_inc_rr<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let index = (op >> 4) & 3;
        let value = self.get_rr(index);
        self.idle(bus, value, 1); // the internal cycle drives rr while the IDU increments it
        self.set_rr(index, value.wrapping_add(1));
    }

    /// DEC rr -- 16-bit decrement through the IDU, no flags (GBCTR DEC_rr).
    pub(crate) fn op_dec_rr<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let index = (op >> 4) & 3;
        let value = self.get_rr(index);
        self.idle(bus, value, -1);
        self.set_rr(index, value.wrapping_sub(1));
    }

    /// ADD HL,rr -- 16-bit add; Z kept, H is the carry out of bit 11 (GBCTR ADD_hl_rr).
    pub(crate) fn op_add_hl_rr<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        self.idle(bus, 0x0000, 0); // the second byte of the add; nothing is driven (GBCTR: $0000)
        let operand = self.get_rr((op >> 4) & 3) as u32;
        let hl = self.hl() as u32;
        let total = hl + operand;
        self.f = (self.f & FLAG_Z)
            | if (hl & 0xFFF) + (operand & 0xFFF) > 0xFFF {
                FLAG_H
            } else {
                0
            }
            | if total > 0xFFFF { FLAG_C } else { 0 };
        self.set_hl(total as u16);
    }

    /// ADD SP,e -- SP plus a signed byte; flags from the low-byte add (GBCTR ADD_sp_e).
    pub(crate) fn op_add_sp_e<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let offset = self.read_pc(bus);
        self.idle(bus, 0x0000, 0); // the ALU adds the low byte
        self.idle(bus, 0x0000, 0); // and finishes the high byte
                                   // Z = N = 0; H and C come from the unsigned add of the low bytes (Pan Docs)
        self.f = if (self.sp & 0xF) + (offset & 0xF) as u16 > 0xF {
            FLAG_H
        } else {
            0
        } | if (self.sp & 0xFF) + offset as u16 > 0xFF {
            FLAG_C
        } else {
            0
        };
        self.sp = self.sp.wrapping_add(signed(offset) as u16);
    }

    /// DAA -- decimal-adjust A after BCD arithmetic (GBCTR DAA; U2, Mooneye instr/daa).
    pub(crate) fn op_daa<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        let mut a = self.a as i32;
        let mut carry = self.f & FLAG_C;
        if self.f & FLAG_N != 0 {
            // after a subtraction: undo the borrows, C stays as it was
            if self.f & FLAG_C != 0 {
                a -= 0x60;
            }
            if self.f & FLAG_H != 0 {
                a -= 0x06;
            }
        } else {
            // after an addition: add the missing corrections, and set C past $99
            if self.f & FLAG_C != 0 || a > 0x99 {
                a += 0x60;
                carry = FLAG_C;
            }
            if self.f & FLAG_H != 0 || self.a & 0x0F > 0x09 {
                a += 0x06;
            }
        }
        self.a = a as u8;
        self.f = (self.f & FLAG_N) | zero_flag(self.a as u32) | carry; // H is always cleared
    }

    /// CPL -- complement A (GBCTR CPL).
    pub(crate) fn op_cpl<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        self.a ^= 0xFF;
        self.f |= FLAG_N | FLAG_H;
    }

    /// SCF -- set the carry flag (GBCTR SCF).
    pub(crate) fn op_scf<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        self.f = (self.f & FLAG_Z) | FLAG_C;
    }

    /// CCF -- complement the carry flag (GBCTR CCF).
    pub(crate) fn op_ccf<B: Bus + ?Sized>(&mut self, _bus: &mut B, _op: u8) {
        self.f = (self.f & FLAG_Z) | ((self.f & FLAG_C) ^ FLAG_C);
    }
}
