//! A minimal host that is not a Game Boy: just enough board to run test ROMs on the core
//! (`harness.py`).
//!
//! Modelled on Gekkio's mock SoC (`gb-research/sm83-cpu-core/hdl/simulation/test_soc.vhd`),
//! which runs the same ROMs on the die model: ROM at $0000-$7FFF with an MBC1-style bank
//! register (enough for the 64 KiB `cpu_instrs.gb`), RAM everywhere else, `LY` ($FF44) always
//! $90, the serial port, and $FF0F / $FFFF wired to the core's `if_` and `ie`. **No timer
//! (TIMA), no PPU**: tests that need them belong to the board (validation.md rung 6).
//!
//! The serial port is a little more than the mock SoC's constant $7F: a write of $81 to SC
//! starts an internal-clock transfer that completes after 8 bits (128 M-cycles each) by
//! appending SB to the collected text, clearing SC bit 7, loading SB with $FF (no peer) and
//! raising IF bit 3.
//!
//! A run ends on `LD B,B` with Mooneye's result (pass = B,C,D,E,H,L = 3,5,8,13,21,34, fail =
//! all $42), on a jump or `JR` to itself (blargg's end of test), or at a cycle budget.
//!
//! Rust: the reference's `Harness` is one object that is both the bus and the owner of the
//! CPU; here [`Harness`] owns the CPU and a [`Board`] (the bus), so the CPU can be stepped
//! with the board as its bus. `run(step=...)` is [`Harness::run_with`].

use crate::core::{Bus, Sm83};
use crate::state::CpuState;

pub const MOONEYE_PASS: [u8; 6] = [3, 5, 8, 13, 21, 34];
/// Internal clock: 8192 bit/s = 512 T-cycles = 128 M-cycles per bit.
pub const SERIAL_CYCLES: u64 = 8 * 128;

pub const SB: u16 = 0xFF01;
pub const SC: u16 = 0xFF02;
pub const DIV: u16 = 0xFF04;
pub const IF_REGISTER: u16 = 0xFF0F;
pub const LY: u16 = 0xFF44;
pub const BOOT_DISABLE: u16 = 0xFF50;

/// The DMG's registers as the boot ROM leaves them: H and C of F follow the checksum.
pub fn post_boot_state(rom: &[u8]) -> CpuState {
    let header_checksum = if rom.len() > 0x014D { rom[0x014D] } else { 0 };
    CpuState {
        a: 0x01,
        f: if header_checksum == 0 { 0x80 } else { 0xB0 },
        c: 0x13,
        e: 0xD8,
        h: 0x01,
        l: 0x4D,
        sp: 0xFFFE,
        pc: 0x0100,
        if_: 0x01, // IF reads $E1 after the boot ROM: VBlank pending
        ..CpuState::default()
    }
}

/// Why a run ended and what it produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunResult {
    /// "ld b,b", "jump to self", "budget" or "locked".
    pub reason: &'static str,
    pub cycles: u64,
    pub instructions: u64,
    /// Everything the ROM wrote to the serial port.
    pub text: String,
    /// B, C, D, E, H, L at the end.
    pub registers: [u8; 6],
}

impl RunResult {
    pub fn mooneye_passed(&self) -> bool {
        self.reason == "ld b,b" && self.registers == MOONEYE_PASS
    }
}

/// ROM, RAM and the serial port: the harness's bus.
pub struct Board {
    pub rom: Vec<u8>,
    pub rom_banks: usize,
    pub bank: usize,
    pub memory: Vec<u8>,
    pub boot: Option<Vec<u8>>,
    pub boot_enabled: bool,
    pub cycles: u64,
    pub serial_text: Vec<u8>,
    pub serial_data: u8,
    pub serial_done_at: Option<u64>,
    /// The M-cycle at which DIV was last reset.
    pub div_origin: u64,
}

impl Board {
    /// One M-cycle passes: the only device is the serial port's bit clock.
    fn tick(&mut self, cpu: &mut Sm83) {
        self.cycles += 1;
        if let Some(done) = self.serial_done_at {
            if self.cycles >= done {
                self.serial_done_at = None;
                self.serial_text.push(self.serial_data);
                self.memory[SC as usize] &= 0x7F;
                self.serial_data = 0xFF; // no peer: the shift register fills with ones
                cpu.request_interrupt(3, false, false);
            }
        }
    }

    /// A byte as the CPU would read it, without making a bus cycle.
    pub fn peek(&self, address: u16) -> u8 {
        let address = address as usize;
        if address < 0x4000 {
            return match (&self.boot, self.boot_enabled && address < 0x100) {
                (Some(boot), true) => boot[address],
                _ => self.rom[address],
            };
        }
        if address < 0x8000 {
            return self.rom[self.bank * 0x4000 + address - 0x4000];
        }
        self.memory[address]
    }
}

impl Bus for Board {
    fn read(&mut self, address: u16, cpu: &mut Sm83) -> u8 {
        self.tick(cpu);
        let index = address as usize;
        if index < 0x4000 {
            if let (Some(boot), true) = (&self.boot, self.boot_enabled && index < 0x100) {
                return boot[index];
            }
            return self.rom[index];
        }
        if index < 0x8000 {
            return self.rom[self.bank * 0x4000 + index - 0x4000];
        }
        match address {
            // the upper byte of a 16-bit counter that ticks every T-cycle
            DIV => (((self.cycles - self.div_origin) * 4) >> 8) as u8,
            LY => 0x90, // vblank forever, as on the mock SoC
            SB => self.serial_data,
            SC => self.memory[SC as usize] | 0x7E,
            IF_REGISTER => cpu.if_ | 0xE0, // the upper three bits read 1 (Mooneye if_ie_registers)
            0xFF00 => 0xFF,                // no button pressed
            _ => self.memory[index],
        }
    }

    fn write(&mut self, address: u16, value: u8, cpu: &mut Sm83) {
        self.tick(cpu);
        if address < 0x8000 {
            if (0x2000..0x4000).contains(&address) {
                // MBC1 bank register: 0 selects bank 1
                let selected = if value & 0x1F != 0 {
                    (value & 0x1F) as usize
                } else {
                    1
                };
                self.bank = selected % self.rom_banks;
            }
            return;
        }
        match address {
            DIV => self.div_origin = self.cycles, // any write resets the divider
            SB => self.serial_data = value,
            SC => {
                self.memory[SC as usize] = value;
                if value & 0x81 == 0x81 {
                    self.serial_done_at = Some(self.cycles + SERIAL_CYCLES);
                }
            }
            IF_REGISTER => cpu.if_ = value & 0x1F,
            BOOT_DISABLE if value != 0 => self.boot_enabled = false,
            _ => self.memory[address as usize] = value,
        }
    }

    fn idle(&mut self, _address: u16, cpu: &mut Sm83) {
        self.tick(cpu);
    }
}

/// ROM, RAM, serial port and an SM83, wired together.
pub struct Harness {
    pub cpu: Sm83,
    pub board: Board,
}

impl Harness {
    pub fn new(rom: &[u8], boot: Option<&[u8]>) -> Self {
        let mut padded = rom.to_vec();
        padded.resize(rom.len().max(0x8000), 0);
        let rom_banks = (padded.len() / 0x4000).max(1);
        let board = Board {
            rom: padded,
            rom_banks,
            bank: 1,
            memory: vec![0; 0x10000],
            boot: boot.map(|b| b.to_vec()),
            boot_enabled: boot.is_some(),
            cycles: 0,
            serial_text: Vec::new(),
            serial_data: 0,
            serial_done_at: None,
            div_origin: 0,
        };
        let mut cpu = Sm83::new();
        if boot.is_none() {
            cpu.restore_state(&post_boot_state(rom));
        } else {
            cpu.reset();
        }
        Harness { cpu, board }
    }

    /// The stop condition of the instruction about to execute, if it is one.
    fn ends_here(&self) -> Option<&'static str> {
        let pc = self.cpu.pc;
        let opcode = self.board.peek(pc);
        if opcode == 0x40 {
            // blargg's own tests execute LD B,B too: only Mooneye's result counts
            let registers = self.registers();
            if registers == MOONEYE_PASS || registers == [0x42; 6] {
                return Some("ld b,b");
            }
        }
        if opcode == 0x18 && self.board.peek(pc.wrapping_add(1)) == 0xFE {
            return Some("jump to self");
        }
        if opcode == 0xC3
            && (self.board.peek(pc.wrapping_add(1)) as u16
                | (self.board.peek(pc.wrapping_add(2)) as u16) << 8)
                == pc
        {
            return Some("jump to self");
        }
        None
    }

    fn registers(&self) -> [u8; 6] {
        let c = &self.cpu;
        [c.b, c.c, c.d, c.e, c.h, c.l]
    }

    /// Step until a stop condition or `max_cycles` M-cycles.
    pub fn run(&mut self, max_cycles: u64) -> RunResult {
        self.run_with(max_cycles, |cpu, board| {
            cpu.step(board);
        })
    }

    /// `run` with `step` in place of `cpu.step` (a trace session's step, for instance).
    pub fn run_with(
        &mut self,
        max_cycles: u64,
        mut step: impl FnMut(&mut Sm83, &mut Board),
    ) -> RunResult {
        let mut instructions = 0;
        let mut reason = "budget";
        while self.board.cycles < max_cycles {
            if self.cpu.locked {
                reason = "locked";
                break;
            }
            if !(self.cpu.halted || self.cpu.stopped) {
                if let Some(found) = self.ends_here() {
                    reason = found;
                    break;
                }
            }
            step(&mut self.cpu, &mut self.board);
            instructions += 1;
        }
        RunResult {
            reason,
            cycles: self.board.cycles,
            instructions,
            // decode("ascii", "replace")
            text: self
                .board
                .serial_text
                .iter()
                .map(|&b| if b < 0x80 { b as char } else { '\u{FFFD}' })
                .collect(),
            registers: self.registers(),
        }
    }
}
