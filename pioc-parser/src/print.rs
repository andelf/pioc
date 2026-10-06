use pioc_core::{
    canonical_bit_name, BitInput, BitIo, Destination, Instruction, Register, Sfr, WaitEvent,
};

use crate::Syntax;

/// Format a decoded word without losing non-canonical or don't-care bits.
///
/// A semantic spelling is used when re-encoding it produces the exact input
/// word. Otherwise the raw word directive preserves the binary bit-for-bit.
pub fn format_decoded(decoded: pioc_core::DecodedInstruction, syntax: Syntax) -> String {
    if decoded.instruction.encode() == Ok(decoded.word) {
        format_instruction(decoded.instruction, syntax)
    } else {
        format!(".word 0x{:04x}", decoded.word)
    }
}

pub fn format_register(register: Register, syntax: Syntax) -> String {
    let address = register.address();
    if address <= u8::MAX as u16 {
        if let Ok(sfr) = Sfr::try_from(address as u8) {
            if let Some(index) = sfr.data_index() {
                return match syntax {
                    Syntax::Legacy => format!("SFR_DATA_REG{index}"),
                    Syntax::Canonical => format!("d{index}"),
                };
            }
            return match syntax {
                Syntax::Legacy => sfr.legacy_name().to_string(),
                Syntax::Canonical => sfr.canonical_name().to_string(),
            };
        }
    }
    format!("0x{address:03x}")
}

fn destination(destination: Destination, syntax: Syntax) -> &'static str {
    match (destination, syntax) {
        (Destination::Accumulator, _) => "a",
        (Destination::Register, Syntax::Legacy) => "F",
        (Destination::Register, Syntax::Canonical) => "reg",
    }
}

fn wait_event(event: WaitEvent, syntax: Syntax) -> &'static str {
    match (event, syntax) {
        (WaitEvent::CoreToHostClear, Syntax::Legacy) => "WB_DATA_SW_MR_0",
        (WaitEvent::CodecDone, Syntax::Legacy) => "WB_BIT_CYC_TAIL_1",
        (WaitEvent::Io0Falling, Syntax::Legacy) => "WB_PORT_I0_FALL",
        (WaitEvent::Io0Rising, Syntax::Legacy) => "WB_PORT_I0_RISE",
        (WaitEvent::HostToCorePending, Syntax::Legacy) => "WB_DATA_MW_SR_1",
        (WaitEvent::Io1Changed, Syntax::Legacy) => "WB_PORT_XOR1_1",
        (WaitEvent::Io0Unchanged, Syntax::Legacy) => "WB_PORT_XOR0_0",
        (WaitEvent::Io0Changed, Syntax::Legacy) => "WB_PORT_XOR0_1",
        (WaitEvent::CoreToHostClear, Syntax::Canonical) => "core_to_host_clear",
        (WaitEvent::CodecDone, Syntax::Canonical) => "codec_done",
        (WaitEvent::Io0Falling, Syntax::Canonical) => "io0_fall",
        (WaitEvent::Io0Rising, Syntax::Canonical) => "io0_rise",
        (WaitEvent::HostToCorePending, Syntax::Canonical) => "host_to_core_pending",
        (WaitEvent::Io1Changed, Syntax::Canonical) => "io1_changed",
        (WaitEvent::Io0Unchanged, Syntax::Canonical) => "io0_unchanged",
        (WaitEvent::Io0Changed, Syntax::Canonical) => "io0_changed",
    }
}

fn bit_input(input: BitInput, syntax: Syntax) -> &'static str {
    match (input, syntax) {
        (BitInput::CarryXorIo0, Syntax::Legacy) => "BI_C_XOR_IN0",
        (BitInput::CodecRx, Syntax::Legacy) => "BI_BIT_RX_I0",
        (BitInput::Io0, Syntax::Legacy) => "BI_PORT_IN0",
        (BitInput::Io1, Syntax::Legacy) => "BI_PORT_IN1",
        (BitInput::CarryXorIo0, Syntax::Canonical) => "carry_xor_io0",
        (BitInput::CodecRx, Syntax::Canonical) => "codec_rx",
        (BitInput::Io0, Syntax::Canonical) => "io0",
        (BitInput::Io1, Syntax::Canonical) => "io1",
    }
}

fn bit_output(endpoint: BitIo, syntax: Syntax) -> &'static str {
    match (endpoint, syntax) {
        (BitIo::Carry, Syntax::Legacy) => "BIO_FLAG_C",
        (BitIo::CodecBit, Syntax::Legacy) => "BO_BIT_TX_O0",
        (BitIo::Io0, Syntax::Legacy) => "BO_PORT_OUT0",
        (BitIo::Io1, Syntax::Legacy) => "BO_PORT_OUT1",
        (BitIo::Carry, Syntax::Canonical) => "carry",
        (BitIo::CodecBit, Syntax::Canonical) => "codec_bit",
        (BitIo::Io0, Syntax::Canonical) => "io0",
        (BitIo::Io1, Syntax::Canonical) => "io1",
    }
}

fn bit_source(endpoint: BitIo, syntax: Syntax) -> &'static str {
    match (endpoint, syntax) {
        (BitIo::Carry, Syntax::Legacy) => "BIO_FLAG_C",
        (BitIo::CodecBit, Syntax::Legacy) => "BI_BIT_RX_I0",
        (BitIo::Io0, Syntax::Legacy) => "BI_PORT_IN0",
        (BitIo::Io1, Syntax::Legacy) => "BI_PORT_IN1",
        _ => bit_output(endpoint, syntax),
    }
}

fn legacy_byte(name: &str, source: Register, dest: Destination) -> String {
    format!(
        "{name} {},{}",
        format_register(source, Syntax::Legacy),
        destination(dest, Syntax::Legacy)
    )
}

fn canonical_byte(name: &str, source: Register, dest: Destination) -> String {
    let source_name = format_register(source, Syntax::Canonical);
    let dest_name = match dest {
        Destination::Accumulator => "a".to_string(),
        Destination::Register => source_name.clone(),
    };
    format!("{name} {dest_name},{source_name}")
}

fn canonical_register_bit(register: Register, bit: u8) -> String {
    canonical_bit_name(register, bit).map_or_else(
        || format!("{}[{bit}]", format_register(register, Syntax::Canonical)),
        str::to_string,
    )
}

pub fn format_instruction(instruction: Instruction, syntax: Syntax) -> String {
    use Instruction::*;
    match (instruction, syntax) {
        (Unknown(word), _) => format!(".word 0x{word:04x}"),
        (Nop, _) => "nop".into(),
        (ClearAccumulator, Syntax::Legacy) => "CLRA".into(),
        (ClearAccumulator, Syntax::Canonical) => "clr a".into(),
        (ClearWatchdog, Syntax::Legacy) => "CLRWDT".into(),
        (ClearWatchdog, Syntax::Canonical) => "wdt.clear".into(),
        (Sleep { mode: 0 }, _) => "sleep".into(),
        (Sleep { mode }, Syntax::Legacy) => format!("SLEEPX {mode}"),
        (Sleep { mode }, Syntax::Canonical) => format!("sleep #{mode}"),
        (Wait(event), Syntax::Legacy) => format!("WAITB {}", wait_event(event, syntax)),
        (Wait(event), Syntax::Canonical) => format!("wait {}", wait_event(event, syntax)),
        (ReadCode { space: 0 }, Syntax::Legacy) => "RDCODE".into(),
        (ReadCode { space }, Syntax::Legacy) => format!("RCODE {space}"),
        (ReadCode { space: 0 }, Syntax::Canonical) => "code.read".into(),
        (ReadCode { space }, Syntax::Canonical) => format!("code.read #{space}"),
        (WriteCode, Syntax::Legacy) => "WRCODE".into(),
        (WriteCode, Syntax::Canonical) => "code.write".into(),
        (Exec { operation }, Syntax::Legacy) => format!("EXEC {operation}"),
        (Exec { operation }, Syntax::Canonical) => format!("exec #{operation}"),
        (BitToCarry(input), Syntax::Legacy) => format!("BCTC {}", bit_input(input, syntax)),
        (BitToCarry(input), Syntax::Canonical) => {
            format!("mov c,bitin[{}]", input as u8)
        }
        (PushAccumulatorStatus, Syntax::Legacy) => "PUSHAS".into(),
        (PushAccumulatorStatus, Syntax::Canonical) => "push {a,flags}".into(),
        (PopAccumulatorStatus, Syntax::Legacy) => "POPAS".into(),
        (PopAccumulatorStatus, Syntax::Canonical) => "pop {a,flags}".into(),
        (PushIndirect1, Syntax::Legacy) => "PUSHA2".into(),
        (PushIndirect1, Syntax::Canonical) => "push {p1,page}".into(),
        (PopIndirect1, Syntax::Legacy) => "POPA2".into(),
        (PopIndirect1, Syntax::Canonical) => "pop {p1,page}".into(),
        (Return, _) => "ret".into(),
        (ReturnZero, Syntax::Legacy) => "RETZ".into(),
        (ReturnZero, Syntax::Canonical) => "ret z=1".into(),
        (ReturnInterrupt, _) => "reti".into(),
        (Clear(register), Syntax::Legacy) => {
            format!("CLR {}", format_register(register, syntax))
        }
        (Clear(register), Syntax::Canonical) => {
            format!("clr {}", format_register(register, syntax))
        }
        (StoreAccumulator(register), Syntax::Legacy) => {
            format!("MOVA {}", format_register(register, syntax))
        }
        (StoreAccumulator(register), Syntax::Canonical) => {
            format!("mov {},a", format_register(register, syntax))
        }
        (
            Load {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("MOV", source, destination),
        (
            Load {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_byte("mov", source, destination),
        (
            Increment {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("INC", source, destination),
        (
            Increment {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_byte("inc", source, destination),
        (
            Decrement {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("DEC", source, destination),
        (
            Decrement {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_byte("dec", source, destination),
        (
            IncrementSkipZero {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("INCSZ", source, destination),
        (
            IncrementSkipZero {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_byte("inc.skipz", source, destination),
        (
            DecrementSkipZero {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("DECSZ", source, destination),
        (
            DecrementSkipZero {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_byte("dec.skipz", source, destination),
        (
            SwapNibbles {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("SWAP", source, destination),
        (
            SwapNibbles {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_byte("swap.nibbles", source, destination),
        (
            And {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("AND", source, destination),
        (
            And {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_alu("and", source, destination),
        (
            Or {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("IOR", source, destination),
        (
            Or {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_alu("or", source, destination),
        (
            Xor {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("XOR", source, destination),
        (
            Xor {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_alu("xor", source, destination),
        (
            Add {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("ADD", source, destination),
        (
            Add {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_alu("add", source, destination),
        (
            Subtract {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("SUB", source, destination),
        (
            Subtract {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => {
            let reg = format_register(source, syntax);
            let dst = if destination == Destination::Accumulator {
                "a"
            } else {
                &reg
            };
            format!("sub {dst},{reg},a")
        }
        (
            RotateLeftCarry {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("RCL", source, destination),
        (
            RotateLeftCarry {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_byte("rolc", source, destination),
        (
            RotateRightCarry {
                source,
                destination,
            },
            Syntax::Legacy,
        ) => legacy_byte("RCR", source, destination),
        (
            RotateRightCarry {
                source,
                destination,
            },
            Syntax::Canonical,
        ) => canonical_byte("rorc", source, destination),
        (ReturnLiteral(value), Syntax::Legacy) => format!("RETL 0x{value:02x}"),
        (ReturnLiteral(value), Syntax::Canonical) => format!("ret #0x{value:02x}"),
        (ReturnLiteralNotZero(value), Syntax::Legacy) => format!("RETLN 0x{value:02x}"),
        (ReturnLiteralNotZero(value), Syntax::Canonical) => format!("ret z=0,#0x{value:02x}"),
        (MoveIndirect0(value), Syntax::Legacy) => format!("MOVIP 0x{value:03x}"),
        (MoveIndirect0(value), Syntax::Canonical) => format!("mov p0,#0x{value:03x}"),
        (MoveIndirect1(value), Syntax::Legacy) => format!("MOVIA 0x{value:03x}"),
        (MoveIndirect1(value), Syntax::Canonical) => format!("mov p1,#0x{value:03x}"),
        (MoveLiteralFast1(value), Syntax::Legacy) => format!("MOVA1F 0x{value:02x}"),
        (MoveLiteralFast1(value), Syntax::Canonical) => format!("mov pins.dir,#0x{value:02x}"),
        (MoveLiteralFast2(value), Syntax::Legacy) => format!("MOVA2F 0x{value:02x}"),
        (MoveLiteralFast2(value), Syntax::Canonical) => format!("mov pins,#0x{value:02x}"),
        (MoveLiteralIndirect1(value), Syntax::Legacy) => format!("MOVA2P 0x{value:02x}"),
        (MoveLiteralIndirect1(value), Syntax::Canonical) => format!("mov [p1+],#0x{value:02x}"),
        (MoveLiteralIndirect0(value), Syntax::Legacy) => format!("MOVA1P 0x{value:02x}"),
        (MoveLiteralIndirect0(value), Syntax::Canonical) => format!("mov [p0],#0x{value:02x}"),
        (MoveLiteral(value), Syntax::Legacy) => format!("MOVL 0x{value:02x}"),
        // `mov.lit` keeps MOVL #0 distinct from the shorter CLRA encoding while
        // ordinary hand-written canonical source may still use `mov a,#0`.
        (MoveLiteral(value), Syntax::Canonical) => format!("mov.lit a,#0x{value:02x}"),
        (AndLiteral(value), Syntax::Legacy) => format!("ANDL 0x{value:02x}"),
        (AndLiteral(value), Syntax::Canonical) => format!("and a,a,#0x{value:02x}"),
        (OrLiteral(value), Syntax::Legacy) => format!("IORL 0x{value:02x}"),
        (OrLiteral(value), Syntax::Canonical) => format!("or a,a,#0x{value:02x}"),
        (XorLiteral(value), Syntax::Legacy) => format!("XORL 0x{value:02x}"),
        (XorLiteral(value), Syntax::Canonical) => format!("xor a,a,#0x{value:02x}"),
        (AddLiteral(value), Syntax::Legacy) => format!("ADDL 0x{value:02x}"),
        (AddLiteral(value), Syntax::Canonical) => format!("add a,a,#0x{value:02x}"),
        (SubtractLiteral(value), Syntax::Legacy) => format!("SUBL 0x{value:02x}"),
        (SubtractLiteral(value), Syntax::Canonical) => format!("sub a,#0x{value:02x},a"),
        (CompareAddLiteral(value), Syntax::Legacy) => format!("CMPLN 0x{value:02x}"),
        (CompareAddLiteral(value), Syntax::Canonical) => format!("flags.add #0x{value:02x},a"),
        (CompareSubtractLiteral(value), Syntax::Legacy) => format!("CMPL 0x{value:02x}"),
        (CompareSubtractLiteral(value), Syntax::Canonical) => format!("flags.sub #0x{value:02x},a"),
        (BitClear { register, bit }, Syntax::Legacy) => {
            format!("BC {},{bit}", format_register(register, syntax))
        }
        (BitClear { register, bit }, Syntax::Canonical) => {
            format!("bclr {}", canonical_register_bit(register, bit))
        }
        (BitSet { register, bit }, Syntax::Legacy) => {
            format!("BS {},{bit}", format_register(register, syntax))
        }
        (BitSet { register, bit }, Syntax::Canonical) => {
            format!("bset {}", canonical_register_bit(register, bit))
        }
        (BitTestSkipClear { register, bit }, Syntax::Legacy) => {
            format!("BTSC {},{bit}", format_register(register, syntax))
        }
        (BitTestSkipClear { register, bit }, Syntax::Canonical) => {
            format!("skip clear,{}", canonical_register_bit(register, bit))
        }
        (BitTestSkipSet { register, bit }, Syntax::Legacy) => {
            format!("BTSS {},{bit}", format_register(register, syntax))
        }
        (BitTestSkipSet { register, bit }, Syntax::Canonical) => {
            format!("skip set,{}", canonical_register_bit(register, bit))
        }
        (BitFromIndirect0 { destination, bit }, Syntax::Legacy) => {
            format!("BP1F {},{bit}", bit_output(destination, syntax))
        }
        (BitFromIndirect0 { destination, bit }, Syntax::Canonical) => {
            format!("mov bitio[{}],p0[{bit}]", destination as u8)
        }
        (BitFromExchange { destination, bit }, Syntax::Legacy) => {
            format!("BP2F {},{bit}", bit_output(destination, syntax))
        }
        (BitFromExchange { destination, bit }, Syntax::Canonical) => {
            format!("mov bitio[{}],xchg[{bit}]", destination as u8)
        }
        (BitToIndirect0 { source, bit }, Syntax::Legacy) => {
            format!("BG1F {},{bit}", bit_source(source, syntax))
        }
        (BitToIndirect0 { source, bit }, Syntax::Canonical) => {
            format!("mov p0[{bit}],bitio[{}]", source as u8)
        }
        (BitToExchange { source, bit }, Syntax::Legacy) => {
            format!("BG2F {},{bit}", bit_source(source, syntax))
        }
        (BitToExchange { source, bit }, Syntax::Canonical) => {
            format!("mov xchg[{bit}],bitio[{}]", source as u8)
        }
        (Jump(target), _) => format!("jmp 0x{target:03x}"),
        (Call(target), _) => format!("call 0x{target:03x}"),
        (JumpNotZero(target), Syntax::Legacy) => format!("JNZ 0x{target:03x}"),
        (JumpNotZero(target), Syntax::Canonical) => format!("jmp nz,0x{target:03x}"),
        (JumpZero(target), Syntax::Legacy) => format!("JZ 0x{target:03x}"),
        (JumpZero(target), Syntax::Canonical) => format!("jmp z,0x{target:03x}"),
        (JumpNotCarry(target), Syntax::Legacy) => format!("JNC 0x{target:03x}"),
        (JumpNotCarry(target), Syntax::Canonical) => format!("jmp nc,0x{target:03x}"),
        (JumpCarry(target), Syntax::Legacy) => format!("JC 0x{target:03x}"),
        (JumpCarry(target), Syntax::Canonical) => format!("jmp c,0x{target:03x}"),
        (CompareJump { literal, target }, Syntax::Legacy) => {
            format!("CMPZ 0x{literal:02x},0x{target:02x}")
        }
        (CompareJump { literal, target }, Syntax::Canonical) => {
            format!("jmp a == #0x{literal:02x},0x{target:02x}")
        }
    }
}

fn canonical_alu(name: &str, source: Register, destination: Destination) -> String {
    let register = format_register(source, Syntax::Canonical);
    let dest = if destination == Destination::Accumulator {
        "a"
    } else {
        &register
    };
    format!("{name} {dest},a,{register}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_names_are_view_specific_and_safe() {
        assert_eq!(
            format_register(Register::low(0x1d), Syntax::Legacy),
            "SFR_CTRL_RD"
        );
        assert_eq!(
            format_register(Register::low(0x1d), Syntax::Canonical),
            "cmd.out"
        );
        assert_eq!(
            format_register(Register::low(0x10), Syntax::Canonical),
            "0x010"
        );
    }

    #[test]
    fn conditions_and_subtraction_are_explicit() {
        assert_eq!(
            format_instruction(Instruction::JumpNotZero(7), Syntax::Canonical),
            "jmp nz,0x007"
        );
        assert_eq!(
            format_instruction(Instruction::SubtractLiteral(2), Syntax::Canonical),
            "sub a,#0x02,a"
        );
    }

    #[test]
    fn every_machine_word_formats_losslessly() {
        for word in 0..=u16::MAX {
            let decoded = Instruction::decode(word, pioc_core::DecodeTarget::Ch32x035Pioc);
            for syntax in [Syntax::Legacy, Syntax::Canonical] {
                let formatted = format_decoded(decoded, syntax);
                let assembled = match syntax {
                    Syntax::Legacy => crate::assemble_legacy(&formatted),
                    Syntax::Canonical => crate::assemble_canonical(&formatted),
                }
                .unwrap_or_else(|error| {
                    panic!("failed to reassemble {word:04x} `{formatted}`: {error}")
                });
                assert_eq!(
                    assembled.words,
                    [word],
                    "failed word {word:04x}: {formatted}"
                );
            }
        }
    }
}
