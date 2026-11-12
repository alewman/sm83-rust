//! Load, store, stack and high-page instructions: LD, LDH, PUSH, POP (`_loads.py`).
//!
//! Each handler runs after the opcode fetch and performs the M-cycles GBCTR lists between the
//! fetch and the next instruction's fetch (docs/timing.md), each with the address the CPU
//! drives.

use crate::core::{Bus, Sm83};
use crate::flags::{FLAG_C, FLAG_H};

pub const HIGH_PAGE: u16 = 0xFF00;

/// A byte as a signed offset.
pub fn signed(byte: u8) -> i16 {
    byte as i8 as i16
}

impl Sm83 {
    /// LD r,r' -- copy a register; one M-cycle, the fetch (GBCTR LD_r_r).
    pub(crate) fn op_ld_r_r<B: Bus + ?Sized>(&mut self, _bus: &mut B, op: u8) {
        self.set_r8((op >> 3) & 7, self.get_r8(op & 7));
    }

    /// LD r,n -- immediate byte into a register (GBCTR LD_r_n).
    pub(crate) fn op_ld_r_n<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let value = self.read_pc(bus);
        self.set_r8((op >> 3) & 7, value);
    }

    /// LD r,(HL) -- load from the address in HL (GBCTR LD_r_hl).
    pub(crate) fn op_ld_r_hl<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let value = self.read(bus, self.hl(), 0);
        self.set_r8((op >> 3) & 7, value);
    }

    /// LD (HL),r -- store a register at the address in HL (GBCTR LD_hl_r).
    pub(crate) fn op_ld_hl_r<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        self.write(bus, self.hl(), self.get_r8(op & 7), 0);
    }

    /// LD (HL),n -- store an immediate byte at HL (GBCTR LD_hl_n).
    pub(crate) fn op_ld_hl_n<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let value = self.read_pc(bus);
        self.write(bus, self.hl(), value, 0);
    }

    /// LD A,(BC)/LD A,(DE) -- load A through BC or DE (GBCTR LD_a_bc, LD_a_de).
    pub(crate) fn op_ld_a_bc_de<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let address = if op & 0x10 != 0 { self.de() } else { self.bc() };
        self.a = self.read(bus, address, 0);
    }

    /// LD (BC),A/LD (DE),A -- store A through BC or DE (GBCTR LD_bc_a, LD_de_a).
    pub(crate) fn op_ld_bc_de_a<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let address = if op & 0x10 != 0 { self.de() } else { self.bc() };
        self.write(bus, address, self.a, 0);
    }

    /// LD A,(HL+)/LD A,(HL-) -- load A, the IDU moves HL (GBCTR LD_a_hli, LD_a_hld).
    pub(crate) fn op_ld_a_hl_step<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let step: i8 = if op & 0x10 != 0 { -1 } else { 1 }; // $2A increments, $3A decrements
        self.a = self.read(bus, self.hl(), step);
        self.set_hl(self.hl().wrapping_add(step as u16));
    }

    /// LD (HL+),A/LD (HL-),A -- store A, the IDU moves HL (GBCTR LD_hli_a, LD_hld_a).
    pub(crate) fn op_ld_hl_step_a<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let step: i8 = if op & 0x10 != 0 { -1 } else { 1 }; // $22 increments, $32 decrements
        self.write(bus, self.hl(), self.a, step);
        self.set_hl(self.hl().wrapping_add(step as u16));
    }

    /// LD rr,nn -- 16-bit immediate into BC, DE, HL or SP (GBCTR LD_rr_nn).
    pub(crate) fn op_ld_rr_nn<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let value = self.read_pc_word(bus);
        self.set_rr((op >> 4) & 3, value);
    }

    /// LD A,(nn) -- load A from an absolute address (GBCTR LD_a_nn).
    pub(crate) fn op_ld_a_nn<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let address = self.read_pc_word(bus);
        self.a = self.read(bus, address, 0);
    }

    /// LD (nn),A -- store A at an absolute address (GBCTR LD_nn_a).
    pub(crate) fn op_ld_nn_a<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let address = self.read_pc_word(bus);
        self.write(bus, address, self.a, 0);
    }

    /// LD (nn),SP -- store SP little-endian at nn and nn+1 (GBCTR LD_nn_sp).
    pub(crate) fn op_ld_nn_sp<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let address = self.read_pc_word(bus);
        self.write(bus, address, self.sp as u8, 1); // the IDU steps WZ to the high byte's address
        self.write(bus, address.wrapping_add(1), (self.sp >> 8) as u8, 0); // (GBCTR C5: data row, not labels)
    }

    /// LD SP,HL -- SP takes HL; the internal cycle drives HL (GBCTR LD_sp_hl).
    pub(crate) fn op_ld_sp_hl<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        self.idle(bus, self.hl(), 0);
        self.sp = self.hl();
    }

    /// LD HL,SP+e -- HL = SP plus a signed byte; low-byte flags (GBCTR LD_hl_sp_e).
    pub(crate) fn op_ld_hl_sp_e<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let offset = self.read_pc(bus);
        self.idle(bus, 0x0000, 0); // the ALU finishes the high byte; nothing is driven (GBCTR: $0000)
        self.set_hl(self.sp.wrapping_add(signed(offset) as u16));
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
    }

    /// LDH (n),A -- store A in the high page at $FF00+n (GBCTR LDH_n_a).
    pub(crate) fn op_ldh_n_a<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let low = self.read_pc(bus);
        self.write(bus, HIGH_PAGE | low as u16, self.a, 0);
    }

    /// LDH A,(n) -- load A from the high page at $FF00+n (GBCTR LDH_a_n).
    pub(crate) fn op_ldh_a_n<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        let low = self.read_pc(bus);
        self.a = self.read(bus, HIGH_PAGE | low as u16, 0);
    }

    /// LDH (C),A -- store A at $FF00+C (GBCTR LDH_c_a).
    pub(crate) fn op_ldh_c_a<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        self.write(bus, HIGH_PAGE | self.c as u16, self.a, 0);
    }

    /// LDH A,(C) -- load A from $FF00+C (GBCTR LDH_a_c).
    pub(crate) fn op_ldh_a_c<B: Bus + ?Sized>(&mut self, bus: &mut B, _op: u8) {
        self.a = self.read(bus, HIGH_PAGE | self.c as u16, 0);
    }

    /// PUSH rr -- push BC, DE, HL or AF (GBCTR PUSH_rr).
    pub(crate) fn op_push<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let value = match (op >> 4) & 3 {
            0 => self.bc(),
            1 => self.de(),
            2 => self.hl(),
            _ => self.af(),
        };
        self.push_word(bus, value);
    }

    /// POP rr -- pop BC, DE, HL or AF; POP AF drops F's low nibble (GBCTR POP_rr; U1).
    pub(crate) fn op_pop<B: Bus + ?Sized>(&mut self, bus: &mut B, op: u8) {
        let value = self.pop_word(bus);
        match (op >> 4) & 3 {
            0 => self.set_bc(value),
            1 => self.set_de(value),
            2 => self.set_hl(value),
            _ => self.set_af(value), // set_af keeps only F bits 7..4
        }
    }
}
