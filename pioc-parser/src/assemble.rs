use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use pioc_core::{
    canonical_bit, BitInput, BitIo, Destination, Instruction, Register, WaitEvent, PROGRAM_WORDS,
};

use crate::{ParseError, Program, Syntax};

#[derive(Debug, Clone)]
struct SourceLine {
    path: String,
    number: usize,
    text: String,
}

#[derive(Debug, Clone)]
enum Statement {
    Empty,
    Equ {
        name: String,
        value: String,
    },
    Org(String),
    Word(String),
    Instruction {
        mnemonic: String,
        operands: Vec<String>,
    },
    End,
}

#[derive(Debug, Clone)]
struct ParsedLine {
    source: SourceLine,
    label: Option<String>,
    statement: Statement,
}

pub fn assemble_legacy(source: &str) -> Result<Program, ParseError> {
    assemble_source(source, Syntax::Legacy)
}

pub fn assemble_canonical(source: &str) -> Result<Program, ParseError> {
    assemble_source(source, Syntax::Canonical)
}

fn assemble_source(source: &str, syntax: Syntax) -> Result<Program, ParseError> {
    let lines = source
        .lines()
        .enumerate()
        .map(|(index, text)| SourceLine {
            path: "<memory>".into(),
            number: index + 1,
            text: text.into(),
        })
        .collect::<Vec<_>>();
    assemble_lines(&lines, syntax)
}

pub fn assemble_file(path: impl AsRef<Path>) -> Result<Program, ParseError> {
    assemble_file_with_syntax(path, Syntax::Legacy)
}

pub fn assemble_file_with_syntax(
    path: impl AsRef<Path>,
    syntax: Syntax,
) -> Result<Program, ParseError> {
    assemble_file_with_dependencies(path, syntax).map(|(program, _)| program)
}

/// Assemble a file and return every source file read through `INCLUDE`.
///
/// The dependency list is ordered by first visit and includes the root file.
/// It is primarily intended for build scripts and procedural macros.
pub fn assemble_file_with_dependencies(
    path: impl AsRef<Path>,
    syntax: Syntax,
) -> Result<(Program, Vec<PathBuf>), ParseError> {
    let path = path.as_ref();
    let mut lines = Vec::new();
    let mut dependencies = Vec::new();
    expand_file(path, &mut lines, &mut dependencies, 0)?;
    Ok((assemble_lines(&lines, syntax)?, dependencies))
}

fn expand_file(
    path: &Path,
    output: &mut Vec<SourceLine>,
    dependencies: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), ParseError> {
    if depth > 16 {
        return Err(ParseError::Io {
            path: path.display().to_string(),
            message: "include nesting exceeds 16 levels".into(),
        });
    }
    let source = fs::read_to_string(path).map_err(|error| ParseError::Io {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let dependency = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !dependencies.contains(&dependency) {
        dependencies.push(dependency);
    }
    for (index, text) in source.lines().enumerate() {
        let content = text.split(';').next().unwrap_or("").trim();
        let mut fields = content.split_whitespace();
        if fields
            .next()
            .is_some_and(|field| field.eq_ignore_ascii_case("INCLUDE"))
        {
            let Some(name) = fields.next() else {
                return source_error(
                    path.display().to_string(),
                    index + 1,
                    "missing include path",
                );
            };
            let name = name.trim_matches(['\'', '"']);
            let include = path.parent().unwrap_or(Path::new(".")).join(name);
            expand_file(&include, output, dependencies, depth + 1)?;
        } else {
            output.push(SourceLine {
                path: path.display().to_string(),
                number: index + 1,
                text: text.into(),
            });
        }
    }
    Ok(())
}

fn assemble_lines(lines: &[SourceLine], syntax: Syntax) -> Result<Program, ParseError> {
    let parsed = lines
        .iter()
        .map(parse_line)
        .collect::<Result<Vec<_>, _>>()?;
    let mut symbols = builtin_symbols();
    let mut pc = 0u16;

    for line in &parsed {
        if let Some(label) = &line.label {
            insert_symbol(&mut symbols, label, pc, line)?;
        }
        match &line.statement {
            Statement::Empty => {}
            Statement::Equ { name, value } => {
                let value = eval(value, &symbols, line)?;
                insert_symbol(&mut symbols, name, value, line)?;
            }
            Statement::Org(value) => pc = eval(value, &symbols, line)?,
            Statement::Word(_) | Statement::Instruction { .. } => {
                pc = pc
                    .checked_add(1)
                    .ok_or_else(|| line_error(line, "program counter overflow"))?;
            }
            Statement::End => break,
        }
    }

    let mut words = Vec::new();
    pc = 0;
    for line in &parsed {
        match &line.statement {
            Statement::Empty | Statement::Equ { .. } => {}
            Statement::Org(value) => {
                let next = eval(value, &symbols, line)?;
                if next < pc {
                    return Err(line_error(line, "ORG cannot move backwards"));
                }
                words.resize(next as usize, 0);
                pc = next;
            }
            Statement::Word(value) => {
                emit(&mut words, pc, eval(value, &symbols, line)?, line)?;
                pc += 1;
            }
            Statement::Instruction { mnemonic, operands } => {
                let instruction = match syntax {
                    Syntax::Legacy => lower_legacy(mnemonic, operands, &symbols, line)?,
                    Syntax::Canonical => lower_canonical(mnemonic, operands, &symbols, line)?,
                };
                let word = instruction
                    .encode()
                    .map_err(|error| line_error(line, &error.to_string()))?;
                emit(&mut words, pc, word, line)?;
                pc += 1;
            }
            Statement::End => break,
        }
    }
    if words.len() > PROGRAM_WORDS {
        return Err(ParseError::Assembly {
            line: 0,
            message: format!(
                "program has {} words; PIOC limit is {PROGRAM_WORDS}",
                words.len()
            ),
        });
    }
    Ok(Program { words })
}

fn emit(words: &mut Vec<u16>, pc: u16, word: u16, line: &ParsedLine) -> Result<(), ParseError> {
    if pc as usize >= PROGRAM_WORDS {
        return Err(line_error(line, "instruction address exceeds 0x7ff"));
    }
    if words.len() < pc as usize {
        words.resize(pc as usize, 0);
    }
    if words.len() == pc as usize {
        words.push(word);
    } else {
        words[pc as usize] = word;
    }
    Ok(())
}

fn parse_line(source: &SourceLine) -> Result<ParsedLine, ParseError> {
    let mut content = source.text.split(';').next().unwrap_or("").trim();
    let mut label = None;
    if let Some(colon) = content.find(':') {
        let candidate = content[..colon].trim();
        if !candidate.is_empty() && !candidate.contains(char::is_whitespace) {
            label = Some(candidate.to_ascii_uppercase());
            content = content[colon + 1..].trim();
        }
    }
    if content.is_empty() {
        return Ok(ParsedLine {
            source: source.clone(),
            label,
            statement: Statement::Empty,
        });
    }

    let fields = content.split_whitespace().collect::<Vec<_>>();
    if fields.len() >= 3 && fields[1].eq_ignore_ascii_case("EQU") {
        return Ok(ParsedLine {
            source: source.clone(),
            label,
            statement: Statement::Equ {
                name: fields[0].to_ascii_uppercase(),
                value: fields[2..].join(" "),
            },
        });
    }
    let mnemonic = fields[0].to_ascii_uppercase();
    let rest = content[fields[0].len()..].trim();
    let statement = match mnemonic.as_str() {
        "ORG" | ".ORIGIN" => Statement::Org(rest.into()),
        "DW" | ".WORD" => Statement::Word(rest.into()),
        "DB" => Statement::Instruction {
            mnemonic: "RETL".into(),
            operands: split_operands(rest),
        },
        "END" | ".END" => Statement::End,
        ".PROGRAM" => Statement::Empty,
        ".DEFINE" => {
            let mut define = rest.split_whitespace();
            let name = define
                .next()
                .ok_or_else(|| line_error_raw(source, "missing .define name"))?;
            let value = define.collect::<Vec<_>>().join(" ");
            if value.is_empty() {
                return Err(line_error_raw(source, "missing .define value"));
            }
            Statement::Equ {
                name: name.to_ascii_uppercase(),
                value,
            }
        }
        "INCLUDE" => {
            return Err(line_error_raw(
                source,
                "INCLUDE is only supported by assemble_file",
            ))
        }
        _ => Statement::Instruction {
            mnemonic,
            operands: split_operands(rest),
        },
    };
    Ok(ParsedLine {
        source: source.clone(),
        label,
        statement,
    })
}

fn split_operands(source: &str) -> Vec<String> {
    if source.is_empty() {
        Vec::new()
    } else {
        source
            .split(',')
            .map(|item| item.trim().to_string())
            .collect()
    }
}

fn lower_legacy(
    mnemonic: &str,
    operands: &[String],
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<Instruction, ParseError> {
    use Instruction::*;
    let one = |index: usize| operand(operands, index, symbols, line);
    let byte_value = |index: usize| -> Result<u8, ParseError> {
        Ok(narrow(one(index)?, 0xff, "literal", line)? as u8)
    };
    let endpoint = |index: usize| -> Result<BitIo, ParseError> {
        Ok(BitIo::from_bits(
            narrow(one(index)?, 3, "bit endpoint", line)? as u8,
        ))
    };
    let register = |index: usize| -> Result<Register, ParseError> {
        let value = one(index)?;
        Register::new(value).ok_or_else(|| line_error(line, "register address exceeds 0x1ff"))
    };
    let direct = |index: usize| -> Result<Register, ParseError> {
        let value = one(index)?;
        if value > 0xff {
            Err(line_error(line, "direct register address exceeds 0xff"))
        } else {
            Ok(Register::low(value as u8))
        }
    };
    let destination = || -> Result<Destination, ParseError> {
        if operands.len() < 2 {
            Ok(Destination::Register)
        } else if operands[1].eq_ignore_ascii_case("A") || operands[1] == "0" {
            Ok(Destination::Accumulator)
        } else {
            Ok(Destination::Register)
        }
    };
    let byte = |kind: u8| -> Result<Instruction, ParseError> {
        let source = direct(0)?;
        let destination = destination()?;
        Ok(match kind {
            4 => Increment {
                source,
                destination,
            },
            5 => Decrement {
                source,
                destination,
            },
            6 => IncrementSkipZero {
                source,
                destination,
            },
            7 => DecrementSkipZero {
                source,
                destination,
            },
            8 => SwapNibbles {
                source,
                destination,
            },
            9 => And {
                source,
                destination,
            },
            10 => Or {
                source,
                destination,
            },
            11 => Xor {
                source,
                destination,
            },
            12 => Add {
                source,
                destination,
            },
            13 => Subtract {
                source,
                destination,
            },
            14 => RotateLeftCarry {
                source,
                destination,
            },
            _ => RotateRightCarry {
                source,
                destination,
            },
        })
    };

    let instruction = match mnemonic {
        "NOP" => Nop,
        "CLRA" => ClearAccumulator,
        "CLRWDT" | "WDT" => ClearWatchdog,
        "SLEEP" | "HALT" => Sleep { mode: 0 },
        "SLEEPX" => Sleep {
            mode: narrow(one(0)?, 3, "sleep mode", line)? as u8,
        },
        "WAITB" => Wait(WaitEvent::from_bits(
            narrow(one(0)?, 7, "wait event", line)? as u8,
        )),
        "WAITRD" => Wait(WaitEvent::CoreToHostClear),
        "WAITWR" | "WAITSPI" => Wait(WaitEvent::HostToCorePending),
        "RDCODE" => ReadCode { space: 0 },
        "RCODE" => ReadCode {
            space: narrow(one(0)?, 3, "code space", line)? as u8,
        },
        "WRCODE" => WriteCode,
        "EXEC" => Exec {
            operation: narrow(one(0)?, 3, "operation", line)? as u8,
        },
        "BCTC" => BitToCarry(BitInput::from_bits(
            narrow(one(0)?, 3, "bit input", line)? as u8
        )),
        "PUSHAS" | "PUSH" => PushAccumulatorStatus,
        "POPAS" | "POP" => PopAccumulatorStatus,
        "PUSHA2" => PushIndirect1,
        "POPA2" => PopIndirect1,
        "RET" | "RETURN" => Return,
        "RETZ" | "RETOK" => ReturnZero,
        "RETIE" | "RETI" => ReturnInterrupt,
        "CLR" | "CLRF" => Clear(direct(0)?),
        "MOVA" => StoreAccumulator(register(0)?),
        "MOV" | "MOVF" => Load {
            source: register(0)?,
            destination: destination()?,
        },
        "INC" | "INCF" => byte(4)?,
        "DEC" | "DECF" => byte(5)?,
        "INCSZ" | "INCFSZ" => byte(6)?,
        "DECSZ" | "DECFSZ" => byte(7)?,
        "SWAP" | "SWAPF" => byte(8)?,
        "AND" | "ANDF" => byte(9)?,
        "IOR" | "IORF" => byte(10)?,
        "XOR" | "XORF" => byte(11)?,
        "ADD" | "ADDF" => byte(12)?,
        "SUB" | "SUBF" => byte(13)?,
        "RCL" | "RCLF" | "RLF" => byte(14)?,
        "RCR" | "RCRF" | "RRF" => byte(15)?,
        "RETL" => ReturnLiteral(byte_value(0)?),
        "RETLN" | "RETER" => ReturnLiteralNotZero(byte_value(0)?),
        "MOVIP" => MoveIndirect0(one(0)?),
        "MOVIA" => MoveIndirect1(one(0)?),
        "MOVA1F" => MoveLiteralFast1(byte_value(0)?),
        "MOVA2F" => MoveLiteralFast2(byte_value(0)?),
        "MOVA2P" => MoveLiteralIndirect1(byte_value(0)?),
        "MOVA1P" => MoveLiteralIndirect0(byte_value(0)?),
        "MOVL" => MoveLiteral(byte_value(0)?),
        "ANDL" => AndLiteral(byte_value(0)?),
        "IORL" | "ORL" => OrLiteral(byte_value(0)?),
        "XORL" => XorLiteral(byte_value(0)?),
        "ADDL" => AddLiteral(byte_value(0)?),
        "SUBL" => SubtractLiteral(byte_value(0)?),
        "CMPLN" => CompareAddLiteral(byte_value(0)?),
        "CMPL" => CompareSubtractLiteral(byte_value(0)?),
        "BC" | "BCF" => BitClear {
            register: direct(0)?,
            bit: narrow(one(1)?, 7, "bit", line)? as u8,
        },
        "BS" | "BSF" => BitSet {
            register: direct(0)?,
            bit: narrow(one(1)?, 7, "bit", line)? as u8,
        },
        "BTSC" | "BTFSC" => BitTestSkipClear {
            register: direct(0)?,
            bit: narrow(one(1)?, 7, "bit", line)? as u8,
        },
        "BTSS" | "BTFSS" => BitTestSkipSet {
            register: direct(0)?,
            bit: narrow(one(1)?, 7, "bit", line)? as u8,
        },
        "BP1F" => BitFromIndirect0 {
            destination: endpoint(0)?,
            bit: narrow(one(1)?, 7, "bit", line)? as u8,
        },
        "BP2F" => BitFromExchange {
            destination: endpoint(0)?,
            bit: narrow(one(1)?, 7, "bit", line)? as u8,
        },
        "BG1F" => BitToIndirect0 {
            source: endpoint(0)?,
            bit: narrow(one(1)?, 7, "bit", line)? as u8,
        },
        "BG2F" => BitToExchange {
            source: endpoint(0)?,
            bit: narrow(one(1)?, 7, "bit", line)? as u8,
        },
        "JMP" | "GOTO" => Jump(one(0)?),
        "CALL" => Call(one(0)?),
        "JNZ" => JumpNotZero(one(0)?),
        "JZ" => JumpZero(one(0)?),
        "JNC" => JumpNotCarry(one(0)?),
        "JC" => JumpCarry(one(0)?),
        "CMPZ" => CompareJump {
            literal: narrow(one(0)?, 0x7f, "compare literal", line)? as u8,
            target: narrow(one(1)?, 0xff, "compare target", line)? as u8,
        },
        _ => return Err(line_error(line, &format!("unknown mnemonic {mnemonic}"))),
    };
    Ok(instruction)
}

fn lower_canonical(
    mnemonic: &str,
    operands: &[String],
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<Instruction, ParseError> {
    use Instruction::*;
    let value = |index: usize| operand(operands, index, symbols, line);
    let reg =
        |index: usize| canonical_register(operand_text(operands, index, line)?, symbols, line);
    let destination = |text: &str, source: Register| -> Result<Destination, ParseError> {
        if text.eq_ignore_ascii_case("a") {
            Ok(Destination::Accumulator)
        } else if canonical_register(text, symbols, line)? == source {
            Ok(Destination::Register)
        } else {
            Err(line_error(
                line,
                "destination must be a or the source register",
            ))
        }
    };

    match mnemonic {
        "NOP" => Ok(Nop),
        "WDT.CLEAR" => Ok(ClearWatchdog),
        "SLEEP" => Ok(Sleep {
            mode: if operands.is_empty() {
                0
            } else {
                narrow(value(0)?, 3, "sleep mode", line)? as u8
            },
        }),
        "WAIT" => Ok(Wait(parse_wait_event(
            operand_text(operands, 0, line)?,
            line,
        )?)),
        "CODE.READ" => Ok(ReadCode {
            space: if operands.is_empty() {
                0
            } else {
                narrow(value(0)?, 3, "code space", line)? as u8
            },
        }),
        "CODE.WRITE" => Ok(WriteCode),
        "EXEC" => Ok(Exec {
            operation: narrow(value(0)?, 3, "operation", line)? as u8,
        }),
        "PUSH" => {
            let joined = operands.join(",").to_ascii_lowercase();
            if joined.contains("p1") {
                Ok(PushIndirect1)
            } else {
                Ok(PushAccumulatorStatus)
            }
        }
        "POP" => {
            let joined = operands.join(",").to_ascii_lowercase();
            if joined.contains("p1") {
                Ok(PopIndirect1)
            } else {
                Ok(PopAccumulatorStatus)
            }
        }
        "RETI" => Ok(ReturnInterrupt),
        "RET" => {
            if operands.is_empty() {
                Ok(Return)
            } else if operands[0].eq_ignore_ascii_case("z=1") {
                Ok(ReturnZero)
            } else if operands[0].eq_ignore_ascii_case("z=0") {
                Ok(ReturnLiteralNotZero(eval8(
                    operand_text(operands, 1, line)?,
                    symbols,
                    line,
                )?))
            } else {
                Ok(ReturnLiteral(eval8(&operands[0], symbols, line)?))
            }
        }
        "MOV" => lower_canonical_mov(operands, symbols, line),
        "CLR" => {
            let target = operand_text(operands, 0, line)?;
            if target.eq_ignore_ascii_case("a") {
                Ok(ClearAccumulator)
            } else {
                Ok(Clear(canonical_register(target, symbols, line)?))
            }
        }
        "MOV.LIT" => {
            expect_len(operands, 2, line)?;
            if !operands[0].eq_ignore_ascii_case("a") {
                return Err(line_error(line, "mov.lit destination must be a"));
            }
            Ok(MoveLiteral(eval8(&operands[1], symbols, line)?))
        }
        "INC" | "DEC" | "INC.SKIPZ" | "DEC.SKIPZ" | "SWAP.NIBBLES" | "ROLC" | "RORC" => {
            expect_len(operands, 2, line)?;
            let source = reg(1)?;
            let destination = destination(&operands[0], source)?;
            Ok(match mnemonic {
                "INC" => Increment {
                    source,
                    destination,
                },
                "DEC" => Decrement {
                    source,
                    destination,
                },
                "INC.SKIPZ" => IncrementSkipZero {
                    source,
                    destination,
                },
                "DEC.SKIPZ" => DecrementSkipZero {
                    source,
                    destination,
                },
                "SWAP.NIBBLES" => SwapNibbles {
                    source,
                    destination,
                },
                "ROLC" => RotateLeftCarry {
                    source,
                    destination,
                },
                _ => RotateRightCarry {
                    source,
                    destination,
                },
            })
        }
        "AND" | "OR" | "XOR" | "ADD" => {
            expect_len(operands, 3, line)?;
            if operands[2].trim().starts_with('#') {
                let immediate = eval8(&operands[2], symbols, line)?;
                return Ok(match mnemonic {
                    "AND" => AndLiteral(immediate),
                    "OR" => OrLiteral(immediate),
                    "XOR" => XorLiteral(immediate),
                    _ => AddLiteral(immediate),
                });
            }
            let source = reg(2)?;
            let destination = destination(&operands[0], source)?;
            Ok(match mnemonic {
                "AND" => And {
                    source,
                    destination,
                },
                "OR" => Or {
                    source,
                    destination,
                },
                "XOR" => Xor {
                    source,
                    destination,
                },
                _ => Add {
                    source,
                    destination,
                },
            })
        }
        "SUB" => {
            expect_len(operands, 3, line)?;
            if operands[1].trim().starts_with('#') {
                Ok(SubtractLiteral(eval8(&operands[1], symbols, line)?))
            } else {
                let source = reg(1)?;
                Ok(Subtract {
                    source,
                    destination: destination(&operands[0], source)?,
                })
            }
        }
        "FLAGS.ADD" => Ok(CompareAddLiteral(eval8(
            operand_text(operands, 0, line)?,
            symbols,
            line,
        )?)),
        "FLAGS.SUB" => Ok(CompareSubtractLiteral(eval8(
            operand_text(operands, 0, line)?,
            symbols,
            line,
        )?)),
        "BCLR" | "BSET" => {
            let (register, bit) =
                parse_register_bit(operand_text(operands, 0, line)?, symbols, line)?;
            if mnemonic == "BCLR" {
                Ok(BitClear { register, bit })
            } else {
                Ok(BitSet { register, bit })
            }
        }
        "SKIP" => {
            expect_len(operands, 2, line)?;
            let (register, bit) = parse_register_bit(&operands[1], symbols, line)?;
            if operands[0].eq_ignore_ascii_case("clear") {
                Ok(BitTestSkipClear { register, bit })
            } else if operands[0].eq_ignore_ascii_case("set") {
                Ok(BitTestSkipSet { register, bit })
            } else {
                Err(line_error(line, "skip condition must be clear or set"))
            }
        }
        "JMP" => lower_canonical_jump(operands, symbols, line),
        "CALL" => Ok(Call(value(0)?)),
        _ => Err(line_error(
            line,
            &format!("unknown canonical mnemonic {mnemonic}"),
        )),
    }
}

fn lower_canonical_mov(
    operands: &[String],
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<Instruction, ParseError> {
    use Instruction::*;
    expect_len(operands, 2, line)?;
    let dst = operands[0].trim();
    let src = operands[1].trim();
    let dst_lower = dst.to_ascii_lowercase();
    let src_lower = src.to_ascii_lowercase();

    if dst_lower == "c" && src_lower.starts_with("bitin[") {
        return Ok(BitToCarry(BitInput::from_bits(parse_index(
            src, symbols, line,
        )?)));
    }
    if dst_lower.starts_with("bitio[") {
        let endpoint = BitIo::from_bits(parse_index(dst, symbols, line)?);
        let (register, bit) = parse_register_bit(src, symbols, line)?;
        return match register.address() {
            0x04 => Ok(BitFromIndirect0 {
                destination: endpoint,
                bit,
            }),
            0x1f => Ok(BitFromExchange {
                destination: endpoint,
                bit,
            }),
            _ => Err(line_error(line, "bitio source must be p0 or xchg")),
        };
    }
    if src_lower.starts_with("bitio[") {
        let endpoint = BitIo::from_bits(parse_index(src, symbols, line)?);
        let (register, bit) = parse_register_bit(dst, symbols, line)?;
        return match register.address() {
            0x04 => Ok(BitToIndirect0 {
                source: endpoint,
                bit,
            }),
            0x1f => Ok(BitToExchange {
                source: endpoint,
                bit,
            }),
            _ => Err(line_error(line, "bitio destination must be p0 or xchg")),
        };
    }

    if src.starts_with('#') {
        let immediate = eval(src, symbols, line)?;
        return Ok(match dst_lower.as_str() {
            "a" if immediate == 0 => ClearAccumulator,
            "a" => MoveLiteral(narrow(immediate, 0xff, "literal", line)? as u8),
            "p0" => MoveIndirect0(immediate),
            "p1" => MoveIndirect1(immediate),
            "pins.dir" => MoveLiteralFast1(narrow(immediate, 0xff, "literal", line)? as u8),
            "pins" => MoveLiteralFast2(narrow(immediate, 0xff, "literal", line)? as u8),
            "[p1+]" => MoveLiteralIndirect1(narrow(immediate, 0xff, "literal", line)? as u8),
            "[p0]" => MoveLiteralIndirect0(narrow(immediate, 0xff, "literal", line)? as u8),
            _ if immediate == 0 => Clear(canonical_register(dst, symbols, line)?),
            _ => {
                return Err(line_error(
                    line,
                    "this register has no direct immediate-load encoding",
                ));
            }
        });
    }
    if src_lower == "a" {
        return Ok(StoreAccumulator(canonical_register(dst, symbols, line)?));
    }
    let source = canonical_register(src, symbols, line)?;
    let destination = if dst_lower == "a" {
        Destination::Accumulator
    } else if canonical_register(dst, symbols, line)? == source {
        Destination::Register
    } else {
        return Err(line_error(
            line,
            "mov destination must be a or the source register",
        ));
    };
    Ok(Load {
        source,
        destination,
    })
}

fn lower_canonical_jump(
    operands: &[String],
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<Instruction, ParseError> {
    use Instruction::*;
    if operands.len() == 1 {
        return Ok(Jump(eval(&operands[0], symbols, line)?));
    }
    expect_len(operands, 2, line)?;
    let condition = operands[0].trim().to_ascii_lowercase();
    let target = eval(&operands[1], symbols, line)?;
    Ok(match condition.as_str() {
        "nz" => JumpNotZero(target),
        "z" => JumpZero(target),
        "nc" => JumpNotCarry(target),
        "c" => JumpCarry(target),
        _ if condition.starts_with("a ==") => {
            let literal = condition.split_once("==").expect("guarded").1;
            CompareJump {
                literal: eval8(literal, symbols, line)?,
                target: narrow(target, 0xff, "compare target", line)? as u8,
            }
        }
        _ => return Err(line_error(line, "unknown jump condition")),
    })
}

fn canonical_register(
    name: &str,
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<Register, ParseError> {
    let lower = name.trim().to_ascii_lowercase();
    let address = match lower.as_str() {
        "idata0" => 0x00,
        "idata1" => 0x01,
        "pcl" => 0x02,
        "status" => 0x03,
        "p0" => 0x04,
        "timer0.count" => 0x05,
        "timer0.ctrl" => 0x06,
        "timer0.reload" => 0x07,
        "codec.period" => 0x08,
        "p1" => 0x09,
        "pins.dir" => 0x0a,
        "pins" => 0x0b,
        "codec.ctrl" => 0x0c,
        "sys.ctrl" => 0x1c,
        "cmd.out" => 0x1d,
        "cmd.in" => 0x1e,
        "xchg" => 0x1f,
        _ if lower.starts_with('d') => lower[1..]
            .parse::<u16>()
            .ok()
            .filter(|index| *index < 32)
            .map(|index| 0x20 + index)
            .ok_or_else(|| line_error(line, "data register must be d0..d31"))?,
        _ => eval(name, symbols, line)?,
    };
    Register::new(address).ok_or_else(|| line_error(line, "register address exceeds 0x1ff"))
}

fn parse_register_bit(
    text: &str,
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<(Register, u8), ParseError> {
    let lower = text.trim().to_ascii_lowercase();
    if let Some(field) = canonical_bit(&lower) {
        return Ok(field);
    }
    let open = text
        .rfind('[')
        .ok_or_else(|| line_error(line, "expected register[bit]"))?;
    let close = text
        .rfind(']')
        .ok_or_else(|| line_error(line, "expected closing ]"))?;
    let register = canonical_register(&text[..open], symbols, line)?;
    let bit = narrow(eval(&text[open + 1..close], symbols, line)?, 7, "bit", line)? as u8;
    Ok((register, bit))
}

fn parse_index(
    text: &str,
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<u8, ParseError> {
    let open = text
        .rfind('[')
        .ok_or_else(|| line_error(line, "expected [index]"))?;
    let close = text
        .rfind(']')
        .ok_or_else(|| line_error(line, "expected closing ]"))?;
    Ok(narrow(
        eval(&text[open + 1..close], symbols, line)?,
        3,
        "index",
        line,
    )? as u8)
}

fn parse_wait_event(text: &str, line: &ParsedLine) -> Result<WaitEvent, ParseError> {
    Ok(match text.trim().to_ascii_lowercase().as_str() {
        "core_to_host_clear" => WaitEvent::CoreToHostClear,
        "codec_done" => WaitEvent::CodecDone,
        "io0_fall" => WaitEvent::Io0Falling,
        "io0_rise" => WaitEvent::Io0Rising,
        "host_to_core_pending" => WaitEvent::HostToCorePending,
        "io1_changed" => WaitEvent::Io1Changed,
        "io0_unchanged" => WaitEvent::Io0Unchanged,
        "io0_changed" => WaitEvent::Io0Changed,
        _ => return Err(line_error(line, "unknown wait event")),
    })
}

fn operand_text<'a>(
    operands: &'a [String],
    index: usize,
    line: &ParsedLine,
) -> Result<&'a str, ParseError> {
    operands
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| line_error(line, &format!("missing operand {}", index + 1)))
}

fn expect_len(operands: &[String], expected: usize, line: &ParsedLine) -> Result<(), ParseError> {
    if operands.len() == expected {
        Ok(())
    } else {
        Err(line_error(
            line,
            &format!("expected {expected} operands, got {}", operands.len()),
        ))
    }
}

fn eval8(
    expression: &str,
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<u8, ParseError> {
    Ok(narrow(eval(expression, symbols, line)?, 0xff, "literal", line)? as u8)
}

fn operand(
    operands: &[String],
    index: usize,
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<u16, ParseError> {
    operands
        .get(index)
        .ok_or_else(|| line_error(line, &format!("missing operand {}", index + 1)))
        .and_then(|value| eval(value, symbols, line))
}

fn narrow(value: u16, maximum: u16, name: &str, line: &ParsedLine) -> Result<u16, ParseError> {
    if value <= maximum {
        Ok(value)
    } else {
        Err(line_error(
            line,
            &format!("{name} {value} exceeds {maximum}"),
        ))
    }
}

fn eval(
    expression: &str,
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<u16, ParseError> {
    let expression = expression.trim().trim_start_matches('#');
    if let Some((left, right)) = split_expression(expression, '+') {
        return eval(left, symbols, line)?
            .checked_add(eval(right, symbols, line)?)
            .ok_or_else(|| line_error(line, "expression overflow"));
    }
    if let Some((left, right)) = split_expression(expression, '-') {
        return eval(left, symbols, line)?
            .checked_sub(eval(right, symbols, line)?)
            .ok_or_else(|| line_error(line, "expression underflow"));
    }
    let upper = expression.to_ascii_uppercase();
    if let Some(value) = symbols.get(&upper) {
        return Ok(*value);
    }
    parse_number(expression)
        .ok_or_else(|| line_error(line, &format!("unknown symbol or number {expression}")))
}

fn split_expression(expression: &str, operator: char) -> Option<(&str, &str)> {
    let index = expression.rfind(operator)?;
    Some((&expression[..index], &expression[index + 1..]))
}

fn parse_number(value: &str) -> Option<u16> {
    let value = value.trim();
    if value.len() >= 3
        && (value.starts_with("B'") || value.starts_with("b'"))
        && value.ends_with('\'')
    {
        return u16::from_str_radix(&value[2..value.len() - 1], 2).ok();
    }
    let lower = value.to_ascii_lowercase();
    if let Some(value) = lower.strip_prefix("0x") {
        u16::from_str_radix(value, 16).ok()
    } else if let Some(value) = lower.strip_prefix("0b") {
        u16::from_str_radix(value, 2).ok()
    } else if let Some(value) = lower.strip_suffix('h') {
        u16::from_str_radix(value, 16).ok()
    } else {
        lower.parse().ok()
    }
}

fn builtin_symbols() -> HashMap<String, u16> {
    let mut symbols = HashMap::new();
    for (name, value) in [
        ("SFR_INDIR_PORT", 0x00),
        ("SFR_INDIR_PORT2", 0x01),
        ("SFR_PRG_COUNT", 0x02),
        ("SFR_STATUS_REG", 0x03),
        ("SFR_INDIR_ADDR", 0x04),
        ("SFR_TMR0_COUNT", 0x05),
        ("SFR_TIMER_CTRL", 0x06),
        ("SFR_TMR0_INIT", 0x07),
        ("SFR_BIT_CYCLE", 0x08),
        ("SFR_INDIR_ADDR2", 0x09),
        ("SFR_PORT_DIR", 0x0a),
        ("SFR_PORT_IO", 0x0b),
        ("SFR_BIT_CONFIG", 0x0c),
        ("SFR_SYS_CFG", 0x1c),
        ("SFR_CTRL_RD", 0x1d),
        ("SFR_CTRL_WR", 0x1e),
        ("SFR_DATA_EXCH", 0x1f),
        ("WB_DATA_SW_MR_0", 0),
        ("WB_BIT_CYC_TAIL_1", 1),
        ("WB_PORT_I0_FALL", 2),
        ("WB_PORT_I0_RISE", 3),
        ("WB_DATA_MW_SR_1", 4),
        ("WB_PORT_XOR1_1", 5),
        ("WB_PORT_XOR0_0", 6),
        ("WB_PORT_XOR0_1", 7),
        ("BI_C_XOR_IN0", 0),
        ("BI_BIT_RX_I0", 1),
        ("BI_PORT_IN0", 2),
        ("BI_PORT_IN1", 3),
        ("BIO_FLAG_C", 0),
        ("BO_BIT_TX_O0", 1),
        ("BO_PORT_OUT0", 2),
        ("BO_PORT_OUT1", 3),
    ] {
        symbols.insert(name.into(), value);
    }
    for index in 0..32 {
        symbols.insert(format!("SFR_DATA_REG{index}"), 0x20 + index as u16);
    }
    symbols
}

fn insert_symbol(
    symbols: &mut HashMap<String, u16>,
    name: &str,
    value: u16,
    line: &ParsedLine,
) -> Result<(), ParseError> {
    let name = name.to_ascii_uppercase();
    match symbols.get(&name) {
        Some(existing) if *existing == value => Ok(()),
        Some(_) => Err(line_error(line, &format!("duplicate symbol {name}"))),
        None => {
            symbols.insert(name, value);
            Ok(())
        }
    }
}

fn line_error(line: &ParsedLine, message: &str) -> ParseError {
    ParseError::Source {
        path: line.source.path.clone(),
        line: line.source.number,
        message: message.into(),
    }
}

fn line_error_raw(line: &SourceLine, message: &str) -> ParseError {
    ParseError::Source {
        path: line.path.clone(),
        line: line.number,
        message: message.into(),
    }
}

fn source_error<T>(path: String, line: usize, message: &str) -> Result<T, ParseError> {
    Err(ParseError::Source {
        path,
        line,
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_labels_aliases_and_directives() {
        let source = r#"
            ORG 0
            DW 0
        START: MOVL 0x12
               MOVA 0x20
               JNZ START
               END
        "#;
        let program = assemble_legacy(source).unwrap();
        assert_eq!(program.words, vec![0x0000, 0x2812, 0x1020, 0x3001]);
    }

    #[test]
    fn db_is_the_vendor_retl_pseudo_instruction() {
        let program = assemble_legacy("DB 0x5a\nEND").unwrap();
        assert_eq!(program.words, vec![0x205a]);
    }

    #[test]
    fn rejects_legacy_operands_instead_of_truncating_them() {
        assert!(assemble_legacy("MOVL 0x100").is_err());
        assert!(assemble_legacy("CMPZ 0x80,0").is_err());
        assert!(assemble_legacy("BP1F 4,0").is_err());
    }
}
