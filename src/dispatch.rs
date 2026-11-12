//! Opcode decoding: bit patterns, first match wins, mapped to handlers (`_dispatch.py`).
//!
//! A pattern is eight characters, `0` and `1` for fixed bits and `x` for a field the handler
//! decodes itself. The table is built once and checked to cover all 256 opcodes; the
//! mnemonic text of every opcode is in the generated `opcodes.rs`. Pan Docs
//! `CPU_Instruction_Set.md` gives the four blocks (00xxxxxx, 01xxxxxx, 10xxxxxx, 11xxxxxx).
//!
//! Rust: the reference maps patterns to handler method *names* and looks the methods up with
//! `getattr`; here they map to [`Handler`] values, the table is built at compile time (a rule
//! gap fails the build, as it raises at import in the reference), and [`Sm83::execute`] is
//! the explicit `match` from a value to its method.

use crate::core::{Bus, Sm83};

/// One handler per reference method name (`_op_nop` is [`Handler::Nop`], and so on).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Handler {
    Nop,
    LdNnSp,
    Stop,
    Jr,
    JrCc,
    LdRrNn,
    AddHlRr,
    LdBcDeA,
    LdABcDe,
    LdHlStepA,
    LdAHlStep,
    IncRr,
    DecRr,
    IncHl,
    DecHl,
    LdHlN,
    IncR,
    DecR,
    LdRN,
    Rlca,
    Rrca,
    Rla,
    Rra,
    Daa,
    Cpl,
    Scf,
    Ccf,
    Halt,
    LdHlR,
    LdRHl,
    LdRR,
    Add,
    Adc,
    Sub,
    Sbc,
    And,
    Xor,
    Or,
    Cp,
    RetCc,
    Ret,
    Reti,
    JpHl,
    LdSpHl,
    Pop,
    Push,
    JpCc,
    Jp,
    CallCc,
    Call,
    Rst,
    LdhNA,
    LdhAN,
    LdhCA,
    LdhAC,
    AddSpE,
    LdHlSpE,
    LdNnA,
    LdANn,
    Cb,
    Di,
    Ei,
    Undefined,
}

/// (pattern, handler), most specific first.
pub const RULES: &[(&[u8; 8], Handler)] = &[
    // block 0: miscellaneous, 16-bit, INC/DEC, LD r,n, rotates of A
    (b"00000000", Handler::Nop),
    (b"00001000", Handler::LdNnSp),
    (b"00010000", Handler::Stop),
    (b"00011000", Handler::Jr),
    (b"001xx000", Handler::JrCc),
    (b"00xx0001", Handler::LdRrNn),
    (b"00xx1001", Handler::AddHlRr),
    (b"000x0010", Handler::LdBcDeA),
    (b"000x1010", Handler::LdABcDe),
    (b"001x0010", Handler::LdHlStepA),
    (b"001x1010", Handler::LdAHlStep),
    (b"00xx0011", Handler::IncRr),
    (b"00xx1011", Handler::DecRr),
    (b"00110100", Handler::IncHl),
    (b"00110101", Handler::DecHl),
    (b"00110110", Handler::LdHlN),
    (b"00xxx100", Handler::IncR),
    (b"00xxx101", Handler::DecR),
    (b"00xxx110", Handler::LdRN),
    (b"00000111", Handler::Rlca),
    (b"00001111", Handler::Rrca),
    (b"00010111", Handler::Rla),
    (b"00011111", Handler::Rra),
    (b"00100111", Handler::Daa),
    (b"00101111", Handler::Cpl),
    (b"00110111", Handler::Scf),
    (b"00111111", Handler::Ccf),
    // block 1: LD r,r' with $76 = HALT
    (b"01110110", Handler::Halt),
    (b"01110xxx", Handler::LdHlR),
    (b"01xxx110", Handler::LdRHl),
    (b"01xxxxxx", Handler::LdRR),
    // block 2 and the immediate forms: the eight ALU operations
    (b"10000xxx", Handler::Add),
    (b"11000110", Handler::Add),
    (b"10001xxx", Handler::Adc),
    (b"11001110", Handler::Adc),
    (b"10010xxx", Handler::Sub),
    (b"11010110", Handler::Sub),
    (b"10011xxx", Handler::Sbc),
    (b"11011110", Handler::Sbc),
    (b"10100xxx", Handler::And),
    (b"11100110", Handler::And),
    (b"10101xxx", Handler::Xor),
    (b"11101110", Handler::Xor),
    (b"10110xxx", Handler::Or),
    (b"11110110", Handler::Or),
    (b"10111xxx", Handler::Cp),
    (b"11111110", Handler::Cp),
    // block 3: control flow, stack, high page, CB, interrupts
    (b"110xx000", Handler::RetCc),
    (b"11001001", Handler::Ret),
    (b"11011001", Handler::Reti),
    (b"11101001", Handler::JpHl),
    (b"11111001", Handler::LdSpHl),
    (b"11xx0001", Handler::Pop),
    (b"11xx0101", Handler::Push),
    (b"110xx010", Handler::JpCc),
    (b"11000011", Handler::Jp),
    (b"110xx100", Handler::CallCc),
    (b"11001101", Handler::Call),
    (b"11xxx111", Handler::Rst),
    (b"11100000", Handler::LdhNA),
    (b"11110000", Handler::LdhAN),
    (b"11100010", Handler::LdhCA),
    (b"11110010", Handler::LdhAC),
    (b"11101000", Handler::AddSpE),
    (b"11111000", Handler::LdHlSpE),
    (b"11101010", Handler::LdNnA),
    (b"11111010", Handler::LdANn),
    (b"11001011", Handler::Cb),
    (b"11110011", Handler::Di),
    (b"11111011", Handler::Ei),
    // the 11 holes: $D3 $DB $DD $E3 $E4 $EB $EC $ED $F4 $FC $FD
    (b"11010011", Handler::Undefined),
    (b"11011011", Handler::Undefined),
    (b"11011101", Handler::Undefined),
    (b"11100011", Handler::Undefined),
    (b"11100100", Handler::Undefined),
    (b"11101011", Handler::Undefined),
    (b"11101100", Handler::Undefined),
    (b"11101101", Handler::Undefined),
    (b"11110100", Handler::Undefined),
    (b"11111100", Handler::Undefined),
    (b"11111101", Handler::Undefined),
];

/// Whether `opcode` fits an eight-character bit pattern.
pub const fn matches(pattern: &[u8; 8], opcode: u8) -> bool {
    let mut i = 0;
    while i < 8 {
        let bit = if (opcode >> (7 - i)) & 1 == 1 {
            b'1'
        } else {
            b'0'
        };
        if pattern[i] != b'x' && pattern[i] != bit {
            return false;
        }
        i += 1;
    }
    true
}

/// The handler of each of the 256 opcodes; fails (at compile time) if one has no rule.
pub const fn build_table() -> [Handler; 256] {
    let mut table = [Handler::Nop; 256];
    let mut opcode = 0;
    while opcode < 256 {
        let mut found = false;
        let mut r = 0;
        while r < RULES.len() {
            if matches(RULES[r].0, opcode as u8) {
                table[opcode] = RULES[r].1;
                found = true;
                break;
            }
            r += 1;
        }
        if !found {
            panic!("no decode rule for an opcode");
        }
        opcode += 1;
    }
    table
}

pub const HANDLERS: [Handler; 256] = build_table();

impl Sm83 {
    /// Run the handler of `opcode` (the reference's `self._table[opcode](opcode)`).
    pub(crate) fn execute<B: Bus + ?Sized>(&mut self, bus: &mut B, opcode: u8) {
        match HANDLERS[opcode as usize] {
            Handler::Nop => self.op_nop(bus, opcode),
            Handler::LdNnSp => self.op_ld_nn_sp(bus, opcode),
            Handler::Stop => self.op_stop(bus, opcode),
            Handler::Jr => self.op_jr(bus, opcode),
            Handler::JrCc => self.op_jr_cc(bus, opcode),
            Handler::LdRrNn => self.op_ld_rr_nn(bus, opcode),
            Handler::AddHlRr => self.op_add_hl_rr(bus, opcode),
            Handler::LdBcDeA => self.op_ld_bc_de_a(bus, opcode),
            Handler::LdABcDe => self.op_ld_a_bc_de(bus, opcode),
            Handler::LdHlStepA => self.op_ld_hl_step_a(bus, opcode),
            Handler::LdAHlStep => self.op_ld_a_hl_step(bus, opcode),
            Handler::IncRr => self.op_inc_rr(bus, opcode),
            Handler::DecRr => self.op_dec_rr(bus, opcode),
            Handler::IncHl => self.op_inc_hl(bus, opcode),
            Handler::DecHl => self.op_dec_hl(bus, opcode),
            Handler::LdHlN => self.op_ld_hl_n(bus, opcode),
            Handler::IncR => self.op_inc_r(bus, opcode),
            Handler::DecR => self.op_dec_r(bus, opcode),
            Handler::LdRN => self.op_ld_r_n(bus, opcode),
            Handler::Rlca => self.op_rlca(bus, opcode),
            Handler::Rrca => self.op_rrca(bus, opcode),
            Handler::Rla => self.op_rla(bus, opcode),
            Handler::Rra => self.op_rra(bus, opcode),
            Handler::Daa => self.op_daa(bus, opcode),
            Handler::Cpl => self.op_cpl(bus, opcode),
            Handler::Scf => self.op_scf(bus, opcode),
            Handler::Ccf => self.op_ccf(bus, opcode),
            Handler::Halt => self.op_halt(bus, opcode),
            Handler::LdHlR => self.op_ld_hl_r(bus, opcode),
            Handler::LdRHl => self.op_ld_r_hl(bus, opcode),
            Handler::LdRR => self.op_ld_r_r(bus, opcode),
            Handler::Add => self.op_add(bus, opcode),
            Handler::Adc => self.op_adc(bus, opcode),
            Handler::Sub => self.op_sub(bus, opcode),
            Handler::Sbc => self.op_sbc(bus, opcode),
            Handler::And => self.op_and(bus, opcode),
            Handler::Xor => self.op_xor(bus, opcode),
            Handler::Or => self.op_or(bus, opcode),
            Handler::Cp => self.op_cp(bus, opcode),
            Handler::RetCc => self.op_ret_cc(bus, opcode),
            Handler::Ret => self.op_ret(bus, opcode),
            Handler::Reti => self.op_reti(bus, opcode),
            Handler::JpHl => self.op_jp_hl(bus, opcode),
            Handler::LdSpHl => self.op_ld_sp_hl(bus, opcode),
            Handler::Pop => self.op_pop(bus, opcode),
            Handler::Push => self.op_push(bus, opcode),
            Handler::JpCc => self.op_jp_cc(bus, opcode),
            Handler::Jp => self.op_jp(bus, opcode),
            Handler::CallCc => self.op_call_cc(bus, opcode),
            Handler::Call => self.op_call(bus, opcode),
            Handler::Rst => self.op_rst(bus, opcode),
            Handler::LdhNA => self.op_ldh_n_a(bus, opcode),
            Handler::LdhAN => self.op_ldh_a_n(bus, opcode),
            Handler::LdhCA => self.op_ldh_c_a(bus, opcode),
            Handler::LdhAC => self.op_ldh_a_c(bus, opcode),
            Handler::AddSpE => self.op_add_sp_e(bus, opcode),
            Handler::LdHlSpE => self.op_ld_hl_sp_e(bus, opcode),
            Handler::LdNnA => self.op_ld_nn_a(bus, opcode),
            Handler::LdANn => self.op_ld_a_nn(bus, opcode),
            Handler::Cb => self.op_cb(bus, opcode),
            Handler::Di => self.op_di(bus, opcode),
            Handler::Ei => self.op_ei(bus, opcode),
            Handler::Undefined => self.op_undefined(bus, opcode),
        }
    }
}
