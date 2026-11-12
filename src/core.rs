//! Registers, the bus, and the shape of `step()`: fetch, interrupt dispatch, HALT (`_core.py`).
//!
//! The bus contract (sm83-python docs/handoff-brief.md "The bus contract"): every M-cycle is
//! one callback, in chip order, with the address the CPU drives, internal cycles included.
//! `idu` holds what the 16-bit increment/decrement unit did to that address (+1, -1 or 0)
//! while the callback runs. A step starts with the opcode fetch and ends after the
//! instruction's last execute cycle, so the next instruction's fetch belongs to the next step
//! (model (a) in sm83-python docs/start-here.md).
//!
//! Differences forced by Rust:
//!
//! - The reference's host passes three callables (and a STOP hook) into the constructor; here
//!   the host implements [`Bus`] and passes it to [`Sm83::step`]. Each bus call also receives
//!   `&mut Sm83`, because a Python callback reaches the CPU through the board's reference to it
//!   (`cpu.request_interrupt`, `cpu.idu`, `cpu.if_`, `cpu.halted`) and a Rust callback cannot
//!   reach the CPU that is calling it any other way. Everything the CPU reads after a callback
//!   returns is read after it, as in the reference, so a callback's changes are seen exactly
//!   where the Python core would see them.
//! - The reference's `CoreMixin` and `SM83` are one struct, [`Sm83`]; each `_*.py` mixin is an
//!   `impl Sm83` block in the module of the same name.
//! - `request_interrupt` raises `ValueError` for a bit outside 0..4; here it panics.

use crate::flags::{FLAG_C, FLAG_Z};
use crate::stop::{StopAction, StopResult};

pub const IE_ADDRESS: u16 = 0xFFFF;
pub const INTERRUPT_BITS: u8 = 0x1F;

/// A 16-bit value from its two bytes.
#[inline]
pub const fn pack(high: u8, low: u8) -> u16 {
    ((high as u16) << 8) | low as u16
}

/// The host's side of the bus: one call per M-cycle, in chip order.
///
/// `cpu.idu` is +1, -1 or 0 while each call runs: what the IDU did to the value on the
/// address bus that cycle. A call may raise interrupts with [`Sm83::request_interrupt`] and
/// read or change any CPU field, as a Python board does through its `cpu` attribute.
pub trait Bus {
    /// One M-cycle that reads. The value of `$FFFF` is ignored: the core owns IE.
    fn read(&mut self, address: u16, cpu: &mut Sm83) -> u8;
    /// One M-cycle that writes. A write to `$FFFF` has already updated `cpu.ie`.
    fn write(&mut self, address: u16, value: u8, cpu: &mut Sm83);
    /// One M-cycle with no access; `address` is what the CPU drives (the reference's default
    /// `idle` does nothing: a host that ignores internal cycles still gets correct timing).
    fn idle(&mut self, _address: u16, _cpu: &mut Sm83) {}
    /// The board's answer to a STOP (`stop.py`). The default is the reference's when no hook
    /// is given: a 2-byte STOP that stops until IF changes.
    fn stop(&mut self, _pending: bool, _cpu: &mut Sm83) -> StopResult {
        StopResult::new(StopAction::Stop)
    }
}

/// A Sharp SM83 core whose memory is a host's [`Bus`] (the reference's `SM83`).
///
/// `ie` and `if_` are fields of the core, which owns `$FFFF`; the board raises interrupts
/// with [`Sm83::request_interrupt`] and keeps `$FF0F` on its own map. A newly constructed
/// CPU has zeroed state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sm83 {
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
    /// The interrupt state before the current fetch, which HALT decides on.
    pub pending_at_fetch: bool,
    /// During a dispatch's low-byte push: the bits raised `late` (after the push sampled IF).
    pub unsampled: Option<u8>,
    /// What the IDU did to the address of the current M-cycle: +1, -1 or 0.
    pub idu: i8,
    /// M-cycles made since construction (the reference's `_cycles`).
    pub(crate) cycles: u64,
}

impl Sm83 {
    // ----- register pairs -------------------------------------------------

    pub fn af(&self) -> u16 {
        pack(self.a, self.f)
    }

    pub fn set_af(&mut self, value: u16) {
        self.a = (value >> 8) as u8;
        self.f = (value & 0xF0) as u8; // F bits 3..0 do not exist (U1; Mooneye bits/reg_f)
    }

    pub fn bc(&self) -> u16 {
        pack(self.b, self.c)
    }

    pub fn set_bc(&mut self, value: u16) {
        self.b = (value >> 8) as u8;
        self.c = value as u8;
    }

    pub fn de(&self) -> u16 {
        pack(self.d, self.e)
    }

    pub fn set_de(&mut self, value: u16) {
        self.d = (value >> 8) as u8;
        self.e = value as u8;
    }

    pub fn hl(&self) -> u16 {
        pack(self.h, self.l)
    }

    pub fn set_hl(&mut self, value: u16) {
        self.h = (value >> 8) as u8;
        self.l = value as u8;
    }

    /// The register named by a 3-bit opcode field (never 6, which is (HL)).
    /// `R8 = ("b", "c", "d", "e", "h", "l", None, "a")`.
    pub(crate) fn get_r8(&self, index: u8) -> u8 {
        match index {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            7 => self.a,
            _ => unreachable!("register field 6 is (HL)"),
        }
    }

    pub(crate) fn set_r8(&mut self, index: u8, value: u8) {
        match index {
            0 => self.b = value,
            1 => self.c = value,
            2 => self.d = value,
            3 => self.e = value,
            4 => self.h = value,
            5 => self.l = value,
            7 => self.a = value,
            _ => unreachable!("register field 6 is (HL)"),
        }
    }

    /// BC, DE, HL or SP: the 2-bit pair field of LD rr,nn / INC rr / ADD HL,rr.
    pub(crate) fn get_rr(&self, index: u8) -> u16 {
        match index {
            0 => self.bc(),
            1 => self.de(),
            2 => self.hl(),
            _ => self.sp,
        }
    }

    pub(crate) fn set_rr(&mut self, index: u8, value: u16) {
        match index {
            0 => self.set_bc(value),
            1 => self.set_de(value),
            2 => self.set_hl(value),
            _ => self.sp = value,
        }
    }

    /// NZ, Z, NC or C: the 2-bit condition field of JR/JP/CALL/RET cc.
    pub(crate) fn condition(&self, code: u8) -> bool {
        let flag = if code < 2 { FLAG_Z } else { FLAG_C };
        (self.f & flag != 0) == (code & 1 != 0)
    }

    // ----- the bus: one callback per M-cycle ------------------------------

    /// One M-cycle that reads. The core owns $FFFF: its value is `ie` (the cycle is still
    /// made, so watchpoints see it).
    pub(crate) fn read<B: Bus + ?Sized>(&mut self, bus: &mut B, address: u16, idu: i8) -> u8 {
        self.idu = idu;
        self.cycles += 1;
        let value = bus.read(address, self);
        if address == IE_ADDRESS {
            self.ie
        } else {
            value
        }
    }

    /// One M-cycle that writes. A write to $FFFF updates `ie` before the callback returns,
    /// so the dispatch's high-byte push can cancel itself (Mooneye ie_push).
    pub(crate) fn write<B: Bus + ?Sized>(&mut self, bus: &mut B, address: u16, value: u8, idu: i8) {
        self.idu = idu;
        self.cycles += 1;
        if address == IE_ADDRESS {
            self.ie = value;
        }
        bus.write(address, value, self);
    }

    /// One M-cycle with no memory access; `address` is what the CPU drives (GBCTR).
    pub(crate) fn idle<B: Bus + ?Sized>(&mut self, bus: &mut B, address: u16, idu: i8) {
        self.idu = idu;
        self.cycles += 1;
        bus.idle(address, self);
    }

    /// Operand fetch: read at PC, the IDU increments PC.
    pub(crate) fn read_pc<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u8 {
        let value = self.read(bus, self.pc, 1);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    /// Two operand fetches, low byte first.
    pub(crate) fn read_pc_word<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u16 {
        let low = self.read_pc(bus);
        pack(self.read_pc(bus), low)
    }

    /// PUSH, CALL, RST: internal cycle at SP, then high and low byte below it.
    pub(crate) fn push_word<B: Bus + ?Sized>(&mut self, bus: &mut B, value: u16) {
        self.idle(bus, self.sp, -1);
        self.sp = self.sp.wrapping_sub(1);
        self.write(bus, self.sp, (value >> 8) as u8, -1);
        self.sp = self.sp.wrapping_sub(1);
        self.write(bus, self.sp, value as u8, 0);
    }

    /// POP, RET: low byte then high byte, the IDU incrementing SP after each read.
    pub(crate) fn pop_word<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u16 {
        let low = self.read(bus, self.sp, 1);
        self.sp = self.sp.wrapping_add(1);
        let high = self.read(bus, self.sp, 1);
        self.sp = self.sp.wrapping_add(1);
        pack(high, low)
    }

    // ----- interrupts, one step at a time ---------------------------------

    /// Raise interrupt `bit` (0 VBlank .. 4 Joypad) in IF. May be called from any bus
    /// callback: it is visible to every decision the core takes after that callback returns,
    /// never to one already taken earlier in the same step.
    ///
    /// `halt_sees`: the board raised it early enough in the M-cycle for a HALT whose fetch
    /// this cycle is to see it, so the HALT does not halt (timing.md "HALT"). The core cannot
    /// place an interrupt within its M-cycle; by default one raised during HALT's fetch is not
    /// seen, as on the die with a timer interrupt.
    ///
    /// `late`: the board raised it after the point early in the M-cycle where a dispatch's
    /// low-byte push samples IF, so that dispatch does not choose it; it stays pending. By
    /// default an interrupt raised in that cycle is chosen, as one raised at its start.
    pub fn request_interrupt(&mut self, bit: u8, halt_sees: bool, late: bool) {
        assert!(bit <= 4, "interrupt bit must be 0..4");
        self.if_ |= 1 << bit;
        if late {
            if let Some(unsampled) = self.unsampled.as_mut() {
                *unsampled |= 1 << bit; // the low-byte push already sampled IF
            }
        }
        if halt_sees && self.interrupt_pending() {
            self.pending_at_fetch = true; // only a HALT fetched in this cycle reads it
        }
    }

    /// IE & IF has a bit among the five interrupt sources set.
    pub fn interrupt_pending(&self) -> bool {
        self.ie & self.if_ & INTERRUPT_BITS != 0
    }

    /// Interrupt dispatch, five M-cycles from the discarded fetch to the vector choice
    /// (docs/timing.md "Interrupt dispatch"; SonoSooS "ISR and NMI"; SameBoy GB_cpu_run).
    ///
    /// The vector is chosen late: IE is sampled after the high-byte push and IF after the low
    /// one, so a push that writes IE ($FFFF) can cancel the dispatch and send PC to $0000
    /// (Mooneye ie_push, hardware-verified on every model). IF is sampled early in the low
    /// push's cycle: a request the board marks `late` there misses the vector
    /// (`request_interrupt`).
    pub(crate) fn dispatch<B: Bus + ?Sized>(&mut self, bus: &mut B) {
        self.halted = false;
        if self.halt_dispatch {
            // EI; HALT with an interrupt pending: the HALT's own M-cycle is the discarded
            // fetch, at the byte after it, with PC not advanced; the IDU then puts PC back onto
            // the HALT itself (die model, rung 5)
            self.halt_dispatch = false;
            self.read(bus, self.pc, 0);
            self.idle(bus, self.pc, -1);
            self.pc = self.pc.wrapping_sub(1);
        } else {
            self.read(bus, self.pc, 1); // M1: the fetch that would start the next instruction, discarded
            self.idle(bus, self.pc.wrapping_add(1), -1); // M2: the IDU puts PC back (docs C2)
        }
        self.idle(bus, self.sp, -1); // M3: SP - 1
        self.sp = self.sp.wrapping_sub(1);
        self.write(bus, self.sp, (self.pc >> 8) as u8, -1); // M4: PCH
        let enabled = self.ie; // IE as the high-byte push left it
        self.sp = self.sp.wrapping_sub(1);
        self.unsampled = Some(0);
        let before = self.if_;
        self.write(bus, self.sp, self.pc as u8, 0); // M5: PCL
        let late = self.unsampled.take().unwrap_or(0);
        // IF as the push left it, without the interrupts raised after the push sampled it
        let queue = enabled & self.if_ & !(late & !before) & INTERRUPT_BITS;
        if queue != 0 {
            let bit = queue.trailing_zeros(); // lowest pending bit wins
            self.if_ &= !(1u8 << bit) | late; // a rise after the sample sets the bit again
            self.pc = 0x40 + 8 * bit as u16;
        } else {
            self.pc = 0x0000; // cancelled: IF untouched (Mooneye ie_push round 1)
        }
        self.ime = false;
    }

    /// Execute one instruction, one interrupt dispatch, or one halted/stopped/locked
    /// M-cycle. Returns the number of M-cycles consumed, which is the number of bus callbacks
    /// made. A halted, stopped or locked CPU makes exactly one idle cycle and returns 1;
    /// the board loops.
    pub fn step<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u32 {
        let start = self.cycles;
        if self.locked {
            self.idle(bus, 0x0000, 0); // the 11 holes: no more fetches, the die drives $0000 (U3, rung 5)
            return 1;
        }
        if self.stopped {
            self.idle(bus, self.pc, 0);
            if (self.if_ & INTERRUPT_BITS) != self.stop_if {
                self.stopped = false;
            }
            return 1;
        }
        let effective_ime = self.ime;
        if self.ei_pending {
            // EI takes effect after the instruction that follows it
            self.ime = true;
            self.ei_pending = false;
        }
        let pending = self.interrupt_pending(); // sampled after the previous cycle's callback
        if self.halted {
            if !pending {
                self.idle(bus, 0x0000, 0); // a halted CPU drives $0000 and nothing else (rung 5)
                return 1;
            }
            // The interrupt flag rose in an earlier cycle: the CPU resumes in this one, with no
            // idle cycle first (dmg-sim wakes in the cycle after IF rises). It dispatches if IME
            // is on, else fetches the byte after the HALT again.
            self.halted = false;
        }
        if effective_ime && pending {
            self.dispatch(bus);
            return (self.cycles - start) as u32;
        }
        self.pending_at_fetch = pending; // HALT decides on the interrupt state before its own fetch
        let opcode = self.read(bus, self.pc, 1); // the fetch
        if self.halt_bug {
            self.halt_bug = false; // the HALT bug: this fetch does not increment PC (U4)
        } else {
            self.pc = self.pc.wrapping_add(1);
        }
        self.execute(bus, opcode);
        (self.cycles - start) as u32
    }
}
