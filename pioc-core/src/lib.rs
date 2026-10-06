#![no_std]
#![doc = "Core instruction, encoding, and register definitions for WCH PIOC/RISC8B."]

mod instruction;
mod register;

pub use instruction::{
    BitInput, BitIo, DecodeTarget, DecodedInstruction, Destination, EncodeError, Instruction,
    WaitEvent,
};
pub use register::{canonical_bit, canonical_bit_name, Register, Sfr, SfrError};

/// CH32X035 PIOC program memory contains 2048 16-bit words.
pub const PROGRAM_WORDS: usize = 2048;

/// Address zero is reserved and is not executed by the PIOC.
pub const RESET_PC: u16 = 1;
