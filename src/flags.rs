//! The SM83 flag register: Z N H C in bits 7-4, bits 3-0 always zero (`_flags.py`).
//!
//! Rust: the reference's `Flags` `IntFlag` is the four constants; nothing
//! else in the core used the enum.

/// Bits of the F register. The low nibble does not exist: it always reads 0 (U1).
pub const FLAG_Z: u8 = 0x80;
pub const FLAG_N: u8 = 0x40;
pub const FLAG_H: u8 = 0x20;
pub const FLAG_C: u8 = 0x10;
