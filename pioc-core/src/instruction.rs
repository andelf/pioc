use core::fmt;

use crate::Register;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecodeTarget {
    /// Generic RISC8B primary encodings from CHRISC8B.
    Generic,
    /// CH32X035 PIOC/eMCU redefinitions used by its examples and VM.
    Ch32x035Pioc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Destination {
    Accumulator,
    Register,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WaitEvent {
    CoreToHostClear = 0,
    CodecDone = 1,
    Io0Falling = 2,
    Io0Rising = 3,
    HostToCorePending = 4,
    Io1Changed = 5,
    Io0Unchanged = 6,
    Io0Changed = 7,
}

impl WaitEvent {
    pub const fn from_bits(value: u8) -> Self {
        match value & 7 {
            0 => Self::CoreToHostClear,
            1 => Self::CodecDone,
            2 => Self::Io0Falling,
            3 => Self::Io0Rising,
            4 => Self::HostToCorePending,
            5 => Self::Io1Changed,
            6 => Self::Io0Unchanged,
            _ => Self::Io0Changed,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitInput {
    CarryXorIo0 = 0,
    CodecRx = 1,
    Io0 = 2,
    Io1 = 3,
}

impl BitInput {
    pub const fn from_bits(value: u8) -> Self {
        match value & 3 {
            0 => Self::CarryXorIo0,
            1 => Self::CodecRx,
            2 => Self::Io0,
            _ => Self::Io1,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitIo {
    Carry = 0,
    CodecBit = 1,
    Io0 = 2,
    Io1 = 3,
}

impl BitIo {
    pub const fn from_bits(value: u8) -> Self {
        match value & 3 {
            0 => Self::Carry,
            1 => Self::CodecBit,
            2 => Self::Io0,
            _ => Self::Io1,
        }
    }
}

/// Semantic PIOC instruction.
///
/// Some variants intentionally share an encoding. They preserve the spelling
/// selected by an assembler. A disassembler selects an interpretation using
/// [`DecodeTarget`], while [`DecodedInstruction::word`] always preserves the
/// original bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Instruction {
    /// Reserved or target-unknown machine word. Re-encodes losslessly.
    Unknown(u16),
    Nop,
    ClearAccumulator,
    ClearWatchdog,
    Sleep {
        mode: u8,
    },
    Wait(WaitEvent),
    ReadCode {
        space: u8,
    },
    WriteCode,
    Exec {
        operation: u8,
    },
    BitToCarry(BitInput),
    PushAccumulatorStatus,
    PopAccumulatorStatus,
    PushIndirect1,
    PopIndirect1,
    Return,
    ReturnZero,
    ReturnInterrupt,
    Clear(Register),
    StoreAccumulator(Register),
    Load {
        source: Register,
        destination: Destination,
    },
    Increment {
        source: Register,
        destination: Destination,
    },
    Decrement {
        source: Register,
        destination: Destination,
    },
    IncrementSkipZero {
        source: Register,
        destination: Destination,
    },
    DecrementSkipZero {
        source: Register,
        destination: Destination,
    },
    SwapNibbles {
        source: Register,
        destination: Destination,
    },
    And {
        source: Register,
        destination: Destination,
    },
    Or {
        source: Register,
        destination: Destination,
    },
    Xor {
        source: Register,
        destination: Destination,
    },
    Add {
        source: Register,
        destination: Destination,
    },
    Subtract {
        source: Register,
        destination: Destination,
    },
    RotateLeftCarry {
        source: Register,
        destination: Destination,
    },
    RotateRightCarry {
        source: Register,
        destination: Destination,
    },
    ReturnLiteral(u8),
    ReturnLiteralNotZero(u8),
    MoveIndirect0(u16),
    MoveIndirect1(u16),
    MoveLiteralFast1(u8),
    MoveLiteralFast2(u8),
    MoveLiteralIndirect1(u8),
    MoveLiteralIndirect0(u8),
    MoveLiteral(u8),
    AndLiteral(u8),
    OrLiteral(u8),
    XorLiteral(u8),
    AddLiteral(u8),
    SubtractLiteral(u8),
    CompareAddLiteral(u8),
    CompareSubtractLiteral(u8),
    BitClear {
        register: Register,
        bit: u8,
    },
    BitSet {
        register: Register,
        bit: u8,
    },
    BitTestSkipClear {
        register: Register,
        bit: u8,
    },
    BitTestSkipSet {
        register: Register,
        bit: u8,
    },
    BitFromIndirect0 {
        destination: BitIo,
        bit: u8,
    },
    BitFromExchange {
        destination: BitIo,
        bit: u8,
    },
    BitToIndirect0 {
        source: BitIo,
        bit: u8,
    },
    BitToExchange {
        source: BitIo,
        bit: u8,
    },
    Jump(u16),
    Call(u16),
    JumpNotZero(u16),
    JumpZero(u16),
    JumpNotCarry(u16),
    JumpCarry(u16),
    CompareJump {
        literal: u8,
        target: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedInstruction {
    pub word: u16,
    pub instruction: Instruction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeError {
    pub field: &'static str,
    pub value: u16,
    pub maximum: u16,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} value {} exceeds {}",
            self.field, self.value, self.maximum
        )
    }
}

fn checked(value: u16, maximum: u16, field: &'static str) -> Result<u16, EncodeError> {
    if value <= maximum {
        Ok(value)
    } else {
        Err(EncodeError {
            field,
            value,
            maximum,
        })
    }
}

const fn destination_bit(destination: Destination) -> u16 {
    match destination {
        Destination::Accumulator => 0,
        Destination::Register => 0x1000,
    }
}

impl Instruction {
    pub fn encode(self) -> Result<u16, EncodeError> {
        use Instruction::*;
        let word = match self {
            Unknown(word) => word,
            Nop => 0x0000,
            ClearAccumulator => 0x0004,
            ClearWatchdog => 0x0008,
            Sleep { mode } => 0x000c | checked(mode as u16, 3, "sleep mode")?,
            Wait(event) => 0x0010 | event as u16,
            ReadCode { space } => 0x0018 | checked(space as u16, 3, "code space")?,
            WriteCode => 0x001c,
            Exec { operation } => 0x001c | checked(operation as u16, 3, "exec operation")?,
            BitToCarry(source) => 0x001c | source as u16,
            PushAccumulatorStatus => 0x0020,
            PopAccumulatorStatus => 0x0024,
            PushIndirect1 => 0x0028,
            PopIndirect1 => 0x002c,
            Return => 0x0030,
            ReturnZero => 0x0034,
            ReturnInterrupt => 0x0038,
            Clear(register) => 0x0100 | checked(register.address(), 0xff, "register")?,
            StoreAccumulator(register) => 0x1000 | checked(register.address(), 0x1ff, "register")?,
            Load {
                source,
                destination,
            } => {
                0x0200
                    | destination_bit(destination)
                    | checked(source.address(), 0x1ff, "register")?
            }
            Increment {
                source,
                destination,
            } => encode_byte(0x04, source, destination)?,
            Decrement {
                source,
                destination,
            } => encode_byte(0x05, source, destination)?,
            IncrementSkipZero {
                source,
                destination,
            } => encode_byte(0x06, source, destination)?,
            DecrementSkipZero {
                source,
                destination,
            } => encode_byte(0x07, source, destination)?,
            SwapNibbles {
                source,
                destination,
            } => encode_byte(0x08, source, destination)?,
            And {
                source,
                destination,
            } => encode_byte(0x09, source, destination)?,
            Or {
                source,
                destination,
            } => encode_byte(0x0a, source, destination)?,
            Xor {
                source,
                destination,
            } => encode_byte(0x0b, source, destination)?,
            Add {
                source,
                destination,
            } => encode_byte(0x0c, source, destination)?,
            Subtract {
                source,
                destination,
            } => encode_byte(0x0d, source, destination)?,
            RotateLeftCarry {
                source,
                destination,
            } => encode_byte(0x0e, source, destination)?,
            RotateRightCarry {
                source,
                destination,
            } => encode_byte(0x0f, source, destination)?,
            ReturnLiteral(value) => 0x2000 | value as u16,
            ReturnLiteralNotZero(value) => 0x2100 | value as u16,
            MoveIndirect0(address) => 0x2200 | checked(address, 0x1ff, "indirect address 0")?,
            MoveIndirect1(address) => 0x2400 | checked(address, 0x3ff, "indirect address 1")?,
            MoveLiteralFast1(value) => 0x2300 | value as u16,
            MoveLiteralFast2(value) => 0x2500 | value as u16,
            MoveLiteralIndirect1(value) => 0x2600 | value as u16,
            MoveLiteralIndirect0(value) => 0x2700 | value as u16,
            MoveLiteral(value) => 0x2800 | value as u16,
            AndLiteral(value) => 0x2900 | value as u16,
            OrLiteral(value) => 0x2a00 | value as u16,
            XorLiteral(value) => 0x2b00 | value as u16,
            AddLiteral(value) => 0x2c00 | value as u16,
            SubtractLiteral(value) => 0x2d00 | value as u16,
            CompareAddLiteral(value) => 0x2e00 | value as u16,
            CompareSubtractLiteral(value) => 0x2f00 | value as u16,
            BitClear { register, bit } => encode_bit(0x4000, register, bit)?,
            BitSet { register, bit } => encode_bit(0x4800, register, bit)?,
            BitTestSkipClear { register, bit } => encode_bit(0x5000, register, bit)?,
            BitTestSkipSet { register, bit } => encode_bit(0x5800, register, bit)?,
            BitFromIndirect0 { destination, bit } => encode_transfer(0x0080, destination, bit)?,
            BitFromExchange { destination, bit } => encode_transfer(0x00a0, destination, bit)?,
            BitToIndirect0 { source, bit } => encode_transfer(0x00c0, source, bit)?,
            BitToExchange { source, bit } => encode_transfer(0x00e0, source, bit)?,
            Jump(target) => 0x6000 | checked(target, 0xfff, "jump target")?,
            Call(target) => 0x7000 | checked(target, 0xfff, "call target")?,
            JumpNotZero(target) => 0x3000 | checked(target, 0x3ff, "jump target")?,
            JumpZero(target) => 0x3400 | checked(target, 0x3ff, "jump target")?,
            JumpNotCarry(target) => 0x3800 | checked(target, 0x3ff, "jump target")?,
            JumpCarry(target) => 0x3c00 | checked(target, 0x3ff, "jump target")?,
            CompareJump { literal, target } => {
                0x8000 | (checked(literal as u16, 0x7f, "compare literal")? << 8) | target as u16
            }
        };
        Ok(word)
    }

    pub const fn decode(word: u16, target: DecodeTarget) -> DecodedInstruction {
        let high = (word >> 8) as u8;
        let low = word as u8;
        let instruction = if word & 0x8000 != 0 {
            Instruction::CompareJump {
                literal: high & 0x7f,
                target: low,
            }
        } else if word & 0xf000 == 0x6000 {
            Instruction::Jump(word & 0x0fff)
        } else if word & 0xf000 == 0x7000 {
            Instruction::Call(word & 0x0fff)
        } else if word & 0xf800 == 0x4000 {
            decode_bit(word, 0)
        } else if word & 0xf800 == 0x4800 {
            decode_bit(word, 1)
        } else if word & 0xf800 == 0x5000 {
            decode_bit(word, 2)
        } else if word & 0xf800 == 0x5800 {
            decode_bit(word, 3)
        } else if word & 0xfc00 == 0x3000 {
            Instruction::JumpNotZero(word & 0x03ff)
        } else if word & 0xfc00 == 0x3400 {
            Instruction::JumpZero(word & 0x03ff)
        } else if word & 0xfc00 == 0x3800 {
            Instruction::JumpNotCarry(word & 0x03ff)
        } else if word & 0xfc00 == 0x3c00 {
            Instruction::JumpCarry(word & 0x03ff)
        } else if high >= 0x20 && high <= 0x2f {
            decode_literal(high, low, word, target)
        } else if word & 0xee00 == 0x0200 {
            Instruction::Load {
                source: reg(word & 0x01ff),
                destination: decode_destination(word),
            }
        } else if word & 0xfe00 == 0x1000 {
            Instruction::StoreAccumulator(reg(word & 0x01ff))
        } else if high == 0x01 {
            Instruction::Clear(Register::low(low))
        } else if high & 0xe0 == 0 && high & 0x0f >= 4 {
            decode_byte(high & 0x0f, low, decode_destination(word))
        } else if high == 0 {
            decode_control(low, target)
        } else {
            Instruction::Unknown(word)
        };
        DecodedInstruction { word, instruction }
    }
}

fn encode_byte(op: u16, source: Register, destination: Destination) -> Result<u16, EncodeError> {
    Ok((op << 8) | destination_bit(destination) | checked(source.address(), 0xff, "register")?)
}

fn encode_bit(base: u16, register: Register, bit: u8) -> Result<u16, EncodeError> {
    Ok(base
        | (checked(bit as u16, 7, "bit")? << 8)
        | checked(register.address(), 0xff, "register")?)
}

fn encode_transfer(base: u16, endpoint: BitIo, bit: u8) -> Result<u16, EncodeError> {
    Ok(base | ((endpoint as u16) << 3) | checked(bit as u16, 7, "bit")?)
}

const fn reg(address: u16) -> Register {
    match Register::new(address) {
        Some(register) => register,
        None => unreachable!(),
    }
}

const fn decode_destination(word: u16) -> Destination {
    if word & 0x1000 == 0 {
        Destination::Accumulator
    } else {
        Destination::Register
    }
}

const fn decode_control(low: u8, target: DecodeTarget) -> Instruction {
    match low {
        0x00..=0x03 => Instruction::Nop,
        0x04..=0x07 => Instruction::ClearAccumulator,
        0x08..=0x0b => Instruction::ClearWatchdog,
        0x0c..=0x0f => Instruction::Sleep { mode: low & 3 },
        0x10..=0x17 => Instruction::Wait(WaitEvent::from_bits(low)),
        0x18..=0x1b => Instruction::ReadCode { space: low & 3 },
        0x1c..=0x1f => match target {
            DecodeTarget::Generic if low == 0x1c => Instruction::WriteCode,
            DecodeTarget::Generic => Instruction::Exec { operation: low & 3 },
            DecodeTarget::Ch32x035Pioc => Instruction::BitToCarry(BitInput::from_bits(low)),
        },
        0x20..=0x23 => Instruction::PushAccumulatorStatus,
        0x24..=0x27 => Instruction::PopAccumulatorStatus,
        0x28..=0x2b => Instruction::PushIndirect1,
        0x2c..=0x2f => Instruction::PopIndirect1,
        0x30..=0x33 => Instruction::Return,
        0x34..=0x37 => Instruction::ReturnZero,
        0x38..=0x3b => Instruction::ReturnInterrupt,
        0x80..=0x9f => Instruction::BitFromIndirect0 {
            destination: BitIo::from_bits(low >> 3),
            bit: low & 7,
        },
        0xa0..=0xbf => Instruction::BitFromExchange {
            destination: BitIo::from_bits(low >> 3),
            bit: low & 7,
        },
        0xc0..=0xdf => Instruction::BitToIndirect0 {
            source: BitIo::from_bits(low >> 3),
            bit: low & 7,
        },
        0xe0..=0xff => Instruction::BitToExchange {
            source: BitIo::from_bits(low >> 3),
            bit: low & 7,
        },
        _ => Instruction::Unknown(low as u16),
    }
}

const fn decode_literal(high: u8, low: u8, word: u16, target: DecodeTarget) -> Instruction {
    match high {
        0x20 => Instruction::ReturnLiteral(low),
        0x21 => Instruction::ReturnLiteralNotZero(low),
        0x22 => Instruction::MoveIndirect0(word & 0x01ff),
        0x23 if matches!(target, DecodeTarget::Ch32x035Pioc) => Instruction::MoveLiteralFast1(low),
        0x23 => Instruction::MoveIndirect0(word & 0x01ff),
        0x24 => Instruction::MoveIndirect1(word & 0x03ff),
        0x25 if matches!(target, DecodeTarget::Ch32x035Pioc) => Instruction::MoveLiteralFast2(low),
        0x26 if matches!(target, DecodeTarget::Ch32x035Pioc) => {
            Instruction::MoveLiteralIndirect1(low)
        }
        0x27 if matches!(target, DecodeTarget::Ch32x035Pioc) => {
            Instruction::MoveLiteralIndirect0(low)
        }
        0x25..=0x27 => Instruction::MoveIndirect1(word & 0x03ff),
        0x28 => Instruction::MoveLiteral(low),
        0x29 => Instruction::AndLiteral(low),
        0x2a => Instruction::OrLiteral(low),
        0x2b => Instruction::XorLiteral(low),
        0x2c => Instruction::AddLiteral(low),
        0x2d => Instruction::SubtractLiteral(low),
        0x2e => Instruction::CompareAddLiteral(low),
        _ => Instruction::CompareSubtractLiteral(low),
    }
}

const fn decode_byte(op: u8, low: u8, destination: Destination) -> Instruction {
    let source = Register::low(low);
    match op {
        0x04 => Instruction::Increment {
            source,
            destination,
        },
        0x05 => Instruction::Decrement {
            source,
            destination,
        },
        0x06 => Instruction::IncrementSkipZero {
            source,
            destination,
        },
        0x07 => Instruction::DecrementSkipZero {
            source,
            destination,
        },
        0x08 => Instruction::SwapNibbles {
            source,
            destination,
        },
        0x09 => Instruction::And {
            source,
            destination,
        },
        0x0a => Instruction::Or {
            source,
            destination,
        },
        0x0b => Instruction::Xor {
            source,
            destination,
        },
        0x0c => Instruction::Add {
            source,
            destination,
        },
        0x0d => Instruction::Subtract {
            source,
            destination,
        },
        0x0e => Instruction::RotateLeftCarry {
            source,
            destination,
        },
        _ => Instruction::RotateRightCarry {
            source,
            destination,
        },
    }
}

const fn decode_bit(word: u16, operation: u8) -> Instruction {
    let register = Register::low(word as u8);
    let bit = ((word >> 8) & 7) as u8;
    match operation {
        0 => Instruction::BitClear { register, bit },
        1 => Instruction::BitSet { register, bit },
        2 => Instruction::BitTestSkipClear { register, bit },
        _ => Instruction::BitTestSkipSet { register, bit },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_condition_codes_are_not_reversed() {
        assert_eq!(
            Instruction::decode(0x3012, DecodeTarget::Ch32x035Pioc).instruction,
            Instruction::JumpNotZero(0x12)
        );
        assert_eq!(
            Instruction::decode(0x3412, DecodeTarget::Ch32x035Pioc).instruction,
            Instruction::JumpZero(0x12)
        );
        assert_eq!(
            Instruction::decode(0x3812, DecodeTarget::Ch32x035Pioc).instruction,
            Instruction::JumpNotCarry(0x12)
        );
        assert_eq!(
            Instruction::decode(0x3c12, DecodeTarget::Ch32x035Pioc).instruction,
            Instruction::JumpCarry(0x12)
        );
    }

    #[test]
    fn pioc_redefinitions_decode_by_target() {
        assert_eq!(
            Instruction::decode(0x2712, DecodeTarget::Ch32x035Pioc).instruction,
            Instruction::MoveLiteralIndirect0(0x12)
        );
        assert_eq!(
            Instruction::decode(0x2712, DecodeTarget::Generic).instruction,
            Instruction::MoveIndirect1(0x312)
        );
        assert_eq!(
            Instruction::decode(0x001f, DecodeTarget::Ch32x035Pioc).instruction,
            Instruction::BitToCarry(BitInput::Io1)
        );
    }

    #[test]
    fn documented_encodings_round_trip_semantically() {
        let cases = [
            Instruction::Nop,
            Instruction::ClearAccumulator,
            Instruction::Wait(WaitEvent::Io0Rising),
            Instruction::MoveLiteral(0xa5),
            Instruction::MoveLiteralIndirect0(0x5a),
            Instruction::JumpNotZero(0x155),
            Instruction::BitSet {
                register: Register::low(0x0b),
                bit: 1,
            },
            Instruction::CompareJump {
                literal: 0x7f,
                target: 0xaa,
            },
        ];
        for instruction in cases {
            let word = instruction.encode().unwrap();
            assert_eq!(
                Instruction::decode(word, DecodeTarget::Ch32x035Pioc).instruction,
                instruction
            );
        }
    }

    #[test]
    fn rejects_out_of_range_operands() {
        assert!(Instruction::Jump(0x1000).encode().is_err());
        assert!(Instruction::BitSet {
            register: Register::low(0),
            bit: 8
        }
        .encode()
        .is_err());
    }

    #[test]
    fn pdf_instruction_table_encodings() {
        let r = Register::low(0x5a);
        let cases = [
            (Instruction::Nop, 0x0000),
            (Instruction::ClearWatchdog, 0x0008),
            (Instruction::Sleep { mode: 0 }, 0x000c),
            (Instruction::Sleep { mode: 2 }, 0x000e),
            (Instruction::Wait(WaitEvent::Io0Rising), 0x0013),
            (Instruction::ReadCode { space: 0 }, 0x0018),
            (Instruction::ReadCode { space: 2 }, 0x001a),
            (Instruction::WriteCode, 0x001c),
            (Instruction::Exec { operation: 2 }, 0x001e),
            (Instruction::PushAccumulatorStatus, 0x0020),
            (Instruction::PopAccumulatorStatus, 0x0024),
            (Instruction::PushIndirect1, 0x0028),
            (Instruction::PopIndirect1, 0x002c),
            (Instruction::Return, 0x0030),
            (Instruction::ReturnZero, 0x0034),
            (Instruction::ReturnInterrupt, 0x0038),
            (Instruction::ClearAccumulator, 0x0004),
            (Instruction::Clear(r), 0x015a),
            (Instruction::StoreAccumulator(Register::low(0x5a)), 0x105a),
            (
                Instruction::Load {
                    source: r,
                    destination: Destination::Accumulator,
                },
                0x025a,
            ),
            (
                Instruction::Increment {
                    source: r,
                    destination: Destination::Register,
                },
                0x145a,
            ),
            (
                Instruction::Decrement {
                    source: r,
                    destination: Destination::Register,
                },
                0x155a,
            ),
            (
                Instruction::IncrementSkipZero {
                    source: r,
                    destination: Destination::Register,
                },
                0x165a,
            ),
            (
                Instruction::DecrementSkipZero {
                    source: r,
                    destination: Destination::Register,
                },
                0x175a,
            ),
            (
                Instruction::SwapNibbles {
                    source: r,
                    destination: Destination::Register,
                },
                0x185a,
            ),
            (
                Instruction::And {
                    source: r,
                    destination: Destination::Register,
                },
                0x195a,
            ),
            (
                Instruction::Or {
                    source: r,
                    destination: Destination::Register,
                },
                0x1a5a,
            ),
            (
                Instruction::Xor {
                    source: r,
                    destination: Destination::Register,
                },
                0x1b5a,
            ),
            (
                Instruction::Add {
                    source: r,
                    destination: Destination::Register,
                },
                0x1c5a,
            ),
            (
                Instruction::Subtract {
                    source: r,
                    destination: Destination::Register,
                },
                0x1d5a,
            ),
            (
                Instruction::RotateLeftCarry {
                    source: r,
                    destination: Destination::Register,
                },
                0x1e5a,
            ),
            (
                Instruction::RotateRightCarry {
                    source: r,
                    destination: Destination::Register,
                },
                0x1f5a,
            ),
            (Instruction::ReturnLiteral(0x5a), 0x205a),
            (Instruction::ReturnLiteralNotZero(0x5a), 0x215a),
            (Instruction::MoveIndirect0(0x5a), 0x225a),
            (Instruction::MoveIndirect1(0x5a), 0x245a),
            (Instruction::MoveLiteralFast1(0x5a), 0x235a),
            (Instruction::MoveLiteralFast2(0x5a), 0x255a),
            (Instruction::MoveLiteralIndirect1(0x5a), 0x265a),
            (Instruction::MoveLiteralIndirect0(0x5a), 0x275a),
            (Instruction::MoveLiteral(0x5a), 0x285a),
            (Instruction::AndLiteral(0x5a), 0x295a),
            (Instruction::OrLiteral(0x5a), 0x2a5a),
            (Instruction::XorLiteral(0x5a), 0x2b5a),
            (Instruction::AddLiteral(0x5a), 0x2c5a),
            (Instruction::SubtractLiteral(0x5a), 0x2d5a),
            (Instruction::CompareAddLiteral(0x5a), 0x2e5a),
            (Instruction::CompareSubtractLiteral(0x5a), 0x2f5a),
            (
                Instruction::BitClear {
                    register: r,
                    bit: 3,
                },
                0x435a,
            ),
            (
                Instruction::BitSet {
                    register: r,
                    bit: 3,
                },
                0x4b5a,
            ),
            (
                Instruction::BitTestSkipClear {
                    register: r,
                    bit: 3,
                },
                0x535a,
            ),
            (
                Instruction::BitTestSkipSet {
                    register: r,
                    bit: 3,
                },
                0x5b5a,
            ),
            (Instruction::BitToCarry(BitInput::Io1), 0x001f),
            (
                Instruction::BitFromIndirect0 {
                    destination: BitIo::Io1,
                    bit: 5,
                },
                0x009d,
            ),
            (
                Instruction::BitFromExchange {
                    destination: BitIo::Io1,
                    bit: 5,
                },
                0x00bd,
            ),
            (
                Instruction::BitToIndirect0 {
                    source: BitIo::Io1,
                    bit: 5,
                },
                0x00dd,
            ),
            (
                Instruction::BitToExchange {
                    source: BitIo::Io1,
                    bit: 5,
                },
                0x00fd,
            ),
            (Instruction::Jump(0x5aa), 0x65aa),
            (Instruction::Call(0x5aa), 0x75aa),
            (Instruction::JumpNotZero(0x155), 0x3155),
            (Instruction::JumpZero(0x155), 0x3555),
            (Instruction::JumpNotCarry(0x155), 0x3955),
            (Instruction::JumpCarry(0x155), 0x3d55),
            (
                Instruction::CompareJump {
                    literal: 0x55,
                    target: 0xaa,
                },
                0xd5aa,
            ),
        ];

        for (instruction, expected) in cases {
            assert_eq!(instruction.encode().unwrap(), expected, "{instruction:?}");
        }
    }
}
