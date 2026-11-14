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

/// Translate legacy WCH assembly into label-preserving canonical source.
pub fn translate_legacy(source: &str) -> Result<String, ParseError> {
    let lines = source
        .lines()
        .enumerate()
        .map(|(index, text)| SourceLine {
            path: "<memory>".into(),
            number: index + 1,
            text: text.into(),
        })
        .collect::<Vec<_>>();
    translate_lines(&lines, None)
}

/// Translate a legacy source file, expanding its `INCLUDE` dependencies.
pub fn translate_legacy_file(path: impl AsRef<Path>) -> Result<String, ParseError> {
    let path = path.as_ref();
    let mut lines = Vec::new();
    let mut dependencies = Vec::new();
    expand_file(path, &mut lines, &mut dependencies, 0)?;
    let program_name = path.file_stem().and_then(|name| name.to_str());
    translate_lines(&lines, program_name)
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

fn translate_lines(lines: &[SourceLine], program_name: Option<&str>) -> Result<String, ParseError> {
    let parsed = lines
        .iter()
        .map(parse_line)
        .collect::<Result<Vec<_>, _>>()?;
    let symbols = collect_symbols(&parsed)?;
    let builtins = builtin_symbols();
    let mut output = String::new();
    if let Some(name) = program_name {
        output.push_str(&format!(".program {}\n\n", name.to_ascii_lowercase()));
    }

    for line in &parsed {
        if Path::new(&line.source.path)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("PIOC_INC.ASM"))
        {
            continue;
        }
        let comment = line
            .source
            .text
            .split_once(';')
            .map(|(_, comment)| comment.trim());
        let body = match &line.statement {
            Statement::Empty => String::new(),
            Statement::Equ { name, value } => {
                let evaluated = eval(value, &symbols, line)?;
                if builtins
                    .get(name)
                    .is_some_and(|builtin| *builtin == evaluated)
                {
                    continue;
                }
                let rhs = translate_equ_value(value, &symbols, line)?;
                let directive = if looks_like_register_alias(value, &symbols, line) {
                    ".alias"
                } else {
                    ".const"
                };
                format!("{directive} {} = {rhs}", name.to_ascii_lowercase())
            }
            Statement::Org(value) => format!(".origin {}", value.to_ascii_lowercase()),
            Statement::Word(value) => format!(".word {}", value.to_ascii_lowercase()),
            Statement::Instruction { mnemonic, operands } => {
                translate_instruction(mnemonic, operands, &symbols, line)?
            }
            Statement::End => ".end".into(),
        };

        if let Some(label) = &line.label {
            output.push_str(&label.to_ascii_lowercase());
            output.push(':');
            if !body.is_empty() {
                output.push('\n');
                output.push_str("    ");
                output.push_str(&body);
            }
        } else if !body.is_empty() {
            output.push_str("    ");
            output.push_str(&body);
        }
        if let Some(comment) = comment.filter(|comment| !comment.is_empty()) {
            if !body.is_empty() || line.label.is_some() {
                output.push_str("    ");
            }
            output.push_str("; ");
            output.push_str(comment);
        }
        output.push('\n');
    }
    // Vendor sources often end with comment and blank lines; a generated file
    // only needs the final newline.
    let trimmed = output.trim_end_matches([' ', '\t', '\n']).len();
    if trimmed != output.len() {
        output.truncate(trimmed);
        output.push('\n');
    }
    Ok(output)
}

fn collect_symbols(parsed: &[ParsedLine]) -> Result<HashMap<String, u16>, ParseError> {
    let mut symbols = builtin_symbols();
    let mut pc = 0u16;
    for line in parsed {
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
    Ok(symbols)
}

fn translate_equ_value(
    value: &str,
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<String, ParseError> {
    if looks_like_register_alias(value, symbols, line) {
        return canonical_register(value, symbols, line)
            .map(|register| crate::format_register(register, Syntax::Canonical));
    }
    Ok(value.to_ascii_lowercase())
}

fn looks_like_register_alias(
    value: &str,
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> bool {
    let upper = value.trim().to_ascii_uppercase();
    upper.starts_with("SFR_")
        || upper.starts_with('D')
            && upper[1..]
                .chars()
                .all(|character| character.is_ascii_digit())
        || matches!(
            upper.as_str(),
            "PTR0"
                | "PTR1"
                | "P0"
                | "P1"
                | "PCL"
                | "STATUS"
                | "TIMER0.COUNT"
                | "TIMER0.CTRL"
                | "TIMER0.RELOAD"
                | "CODEC.PERIOD"
                | "CODEC.CTRL"
                | "PINS.DIR"
                | "PINS"
                | "SYS.CTRL"
                | "CMD.OUT"
                | "CMD.IN"
                | "XCHG"
        ) && canonical_register(value, symbols, line).is_ok()
}

fn translated_register(
    text: &str,
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<String, ParseError> {
    let upper = text.trim().to_ascii_uppercase();
    if upper.starts_with("SFR_") || parse_number(text).is_some() {
        let register = canonical_register(text, symbols, line)?;
        Ok(crate::format_register(register, Syntax::Canonical))
    } else {
        Ok(text.trim().to_ascii_lowercase())
    }
}

fn translated_bit(
    register_text: &str,
    bit_text: &str,
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<String, ParseError> {
    let register = canonical_register(register_text, symbols, line)?;
    let bit = narrow(eval(bit_text, symbols, line)?, 7, "bit", line)? as u8;
    if register.address() == 0x01 {
        return Ok(format!("[ptr1][{}]", bit_text.trim().to_ascii_lowercase()));
    }
    if register_text
        .trim()
        .to_ascii_uppercase()
        .starts_with("SFR_")
    {
        if let Some(name) = pioc_core::canonical_bit_name(register, bit) {
            return Ok(name.into());
        }
    }
    Ok(format!(
        "{}[{}]",
        translated_register(register_text, symbols, line)?,
        bit_text.trim().to_ascii_lowercase()
    ))
}

fn immediate(text: &str) -> String {
    format!(
        "#{}",
        text.trim().trim_start_matches('#').to_ascii_lowercase()
    )
}

fn translate_instruction(
    mnemonic: &str,
    operands: &[String],
    symbols: &HashMap<String, u16>,
    line: &ParsedLine,
) -> Result<String, ParseError> {
    let instruction = lower_legacy(mnemonic, operands, symbols, line)?;
    let operand = |index: usize| operand_text(operands, index, line);
    let reg = |index: usize| translated_register(operand(index)?, symbols, line);
    let bit = || translated_bit(operand(0)?, operand(1)?, symbols, line);
    let target = |index: usize| -> Result<String, ParseError> {
        Ok(operand(index)?.trim().to_ascii_lowercase())
    };
    let byte_destination = |source: &str| -> String {
        if operands
            .get(1)
            .is_some_and(|destination| destination.eq_ignore_ascii_case("A") || destination == "0")
        {
            "a".into()
        } else {
            source.into()
        }
    };

    Ok(match mnemonic {
        "NOP" | "CLRA" | "CLRWDT" | "WDT" | "SLEEP" | "HALT" | "WAITRD" | "WAITWR" | "WAITSPI"
        | "RDCODE" | "WRCODE" | "PUSHAS" | "PUSH" | "POPAS" | "POP" | "PUSHA2" | "POPA2"
        | "RET" | "RETURN" | "RETZ" | "RETOK" | "RETIE" | "RETI" => {
            crate::format_instruction(instruction, Syntax::Canonical)
        }
        "SLEEPX" => format!("sleep {}", immediate(operand(0)?)),
        "WAITB" => crate::format_instruction(instruction, Syntax::Canonical),
        "RCODE" => format!("code.read {}", immediate(operand(0)?)),
        "EXEC" => format!("exec {}", immediate(operand(0)?)),
        "BCTC" => crate::format_instruction(instruction, Syntax::Canonical),
        "CLR" | "CLRF" => format!("clr {}", reg(0)?),
        "MOVA" => {
            let destination = if canonical_register(operand(0)?, symbols, line)?.address() == 0x01 {
                "[ptr1+]".into()
            } else {
                reg(0)?
            };
            format!("mov {destination},a")
        }
        "MOV" | "MOVF" => {
            let source = if canonical_register(operand(0)?, symbols, line)?.address() == 0x01 {
                "[ptr1+]".into()
            } else {
                reg(0)?
            };
            format!("mov {},{source}", byte_destination(&source))
        }
        "INC" | "INCF" | "DEC" | "DECF" | "INCSZ" | "INCFSZ" | "DECSZ" | "DECFSZ" | "SWAP"
        | "SWAPF" | "RCL" | "RCLF" | "RLF" | "RCR" | "RCRF" | "RRF" => {
            let source = reg(0)?;
            let name = match mnemonic {
                "INC" | "INCF" => "inc",
                "DEC" | "DECF" => "dec",
                "INCSZ" | "INCFSZ" => "inc.skipz",
                "DECSZ" | "DECFSZ" => "dec.skipz",
                "SWAP" | "SWAPF" => "swap.nibbles",
                "RCL" | "RCLF" | "RLF" => "rolc",
                _ => "rorc",
            };
            let destination = byte_destination(&source);
            if destination == source {
                format!("{name} {source}")
            } else {
                format!("{name} {destination},{source}")
            }
        }
        "AND" | "ANDF" | "IOR" | "IORF" | "XOR" | "XORF" | "ADD" | "ADDF" | "SUB" | "SUBF" => {
            let source = reg(0)?;
            let destination = byte_destination(&source);
            let name = match mnemonic {
                "AND" | "ANDF" => "and",
                "IOR" | "IORF" => "or",
                "XOR" | "XORF" => "xor",
                "ADD" | "ADDF" => "add",
                _ => "sub",
            };
            if mnemonic == "SUB" || mnemonic == "SUBF" {
                if destination == source {
                    format!("sub {source},a")
                } else {
                    format!("sub a,{source},a")
                }
            } else if destination == source {
                format!("{name} {source},a")
            } else {
                format!("{name} a,{source}")
            }
        }
        "RETL" => format!("ret {}", immediate(operand(0)?)),
        "RETLN" | "RETER" => format!("ret z=0,{}", immediate(operand(0)?)),
        "MOVIP" => format!("mov ptr0,{}", immediate(operand(0)?)),
        "MOVIA" => format!("mov ptr1,{}", immediate(operand(0)?)),
        "MOVA1F" => format!("mov pins.dir,{}", immediate(operand(0)?)),
        "MOVA2F" => format!("mov pins,{}", immediate(operand(0)?)),
        "MOVA2P" => format!("mov [ptr1+],{}", immediate(operand(0)?)),
        "MOVA1P" => format!("mov [ptr0],{}", immediate(operand(0)?)),
        "MOVL" => {
            if matches!(instruction, Instruction::MoveLiteral(0)) {
                // Only zero needs an explicit encoding escape: plain `mov a,#0`
                // intentionally selects the shorter CLRA instruction.
                format!("mov.lit a,{}", immediate(operand(0)?))
            } else {
                format!("mov a,{}", immediate(operand(0)?))
            }
        }
        "ANDL" | "IORL" | "ORL" | "XORL" | "ADDL" => {
            let name = match mnemonic {
                "ANDL" => "and",
                "IORL" | "ORL" => "or",
                "XORL" => "xor",
                _ => "add",
            };
            format!("{name} a,{}", immediate(operand(0)?))
        }
        "SUBL" => format!("sub a,{},a", immediate(operand(0)?)),
        "CMPLN" => format!("flags.add {},a", immediate(operand(0)?)),
        "CMPL" => format!("flags.sub {},a", immediate(operand(0)?)),
        "BC" | "BCF" => format!("bclr {}", bit()?),
        "BS" | "BSF" => format!("bset {}", bit()?),
        "BTSC" | "BTFSC" => format!("skip clear,{}", bit()?),
        "BTSS" | "BTFSS" => format!("skip set,{}", bit()?),
        "BP1F" | "BP2F" | "BG1F" | "BG2F" => {
            crate::format_instruction(instruction, Syntax::Canonical)
        }
        "JMP" | "GOTO" => format!("jmp {}", target(0)?),
        "CALL" => format!("call {}", target(0)?),
        "JNZ" => format!("jmp nz,{}", target(0)?),
        "JZ" => format!("jmp z,{}", target(0)?),
        "JNC" => format!("jmp nc,{}", target(0)?),
        "JC" => format!("jmp c,{}", target(0)?),
        "CMPZ" => format!("jmp a == {},{}", immediate(operand(0)?), target(1)?),
        _ => crate::format_instruction(instruction, Syntax::Canonical),
    })
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
        ".ALIAS" | ".CONST" => {
            let (name, value) = rest
                .split_once('=')
                .map(|(name, value)| (name.trim(), value.trim()))
                .or_else(|| {
                    let mut fields = rest.splitn(2, char::is_whitespace);
                    Some((fields.next()?, fields.next()?.trim()))
                })
                .ok_or_else(|| line_error_raw(source, "expected name = value"))?;
            if name.is_empty() || value.is_empty() {
                return Err(line_error_raw(source, "expected name = value"));
            }
            Statement::Equ {
                name: name.to_ascii_uppercase(),
                value: value.into(),
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
            if joined.contains("ptr1") || joined.contains("p1") {
                Ok(PushIndirect1)
            } else {
                Ok(PushAccumulatorStatus)
            }
        }
        "POP" => {
            let joined = operands.join(",").to_ascii_lowercase();
            if joined.contains("ptr1") || joined.contains("p1") {
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
            if operands.is_empty() || operands.len() > 2 {
                return Err(line_error(line, "expected one or two operands"));
            }
            let (source, destination) = if operands.len() == 1 {
                (reg(0)?, Destination::Register)
            } else {
                let source = reg(1)?;
                (source, destination(&operands[0], source)?)
            };
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
        "XOR"
            if operands.len() == 2
                && operands[0].eq_ignore_ascii_case("c")
                && matches!(
                    operands[1].trim().to_ascii_lowercase().as_str(),
                    "pins.in0" | "pins.input0" | "carry_xor_io0"
                ) =>
        {
            Ok(BitToCarry(BitInput::CarryXorIo0))
        }
        "AND" | "OR" | "XOR" | "ADD" => {
            if operands.len() == 2 && operands[1].trim().starts_with('#') {
                if !operands[0].eq_ignore_ascii_case("a") {
                    return Err(line_error(line, "literal ALU destination must be a"));
                }
                let immediate = eval8(&operands[1], symbols, line)?;
                return Ok(match mnemonic {
                    "AND" => AndLiteral(immediate),
                    "OR" => OrLiteral(immediate),
                    "XOR" => XorLiteral(immediate),
                    _ => AddLiteral(immediate),
                });
            }
            let (source, destination) = if operands.len() == 2 {
                if operands[0].eq_ignore_ascii_case("a") {
                    (reg(1)?, Destination::Accumulator)
                } else if operands[1].eq_ignore_ascii_case("a") {
                    (reg(0)?, Destination::Register)
                } else {
                    return Err(line_error(
                        line,
                        "two-operand ALU form must read or write a",
                    ));
                }
            } else {
                expect_len(operands, 3, line)?;
                let source = reg(2)?;
                (source, destination(&operands[0], source)?)
            };
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
            if operands.len() == 2
                && !operands[0].eq_ignore_ascii_case("a")
                && operands[1].eq_ignore_ascii_case("a")
            {
                return Ok(Subtract {
                    source: reg(0)?,
                    destination: Destination::Register,
                });
            }
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

    if dst_lower == "c" {
        if let Some(input) = canonical_bit_input(&src_lower) {
            return Ok(BitToCarry(input));
        }
        if src_lower.starts_with("bitin[") {
            return Ok(BitToCarry(BitInput::from_bits(parse_index(
                src, symbols, line,
            )?)));
        }
    }
    if let Some(endpoint) = canonical_bit_output(&dst_lower) {
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
            _ => Err(line_error(line, "bit endpoint source must be ptr0 or xchg")),
        };
    }
    if let Some(endpoint) = canonical_bit_source(&src_lower) {
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
            _ => Err(line_error(
                line,
                "bit endpoint destination must be ptr0 or xchg",
            )),
        };
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
            _ => Err(line_error(line, "bitio source must be ptr0 or xchg")),
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
            _ => Err(line_error(line, "bitio destination must be ptr0 or xchg")),
        };
    }

    if src.starts_with('#') {
        let immediate = eval(src, symbols, line)?;
        return Ok(match dst_lower.as_str() {
            "a" if immediate == 0 => ClearAccumulator,
            "a" => MoveLiteral(narrow(immediate, 0xff, "literal", line)? as u8),
            "ptr0" | "p0" => MoveIndirect0(immediate),
            "ptr1" | "p1" => MoveIndirect1(immediate),
            "pins.dir" => MoveLiteralFast1(narrow(immediate, 0xff, "literal", line)? as u8),
            "pins" => MoveLiteralFast2(narrow(immediate, 0xff, "literal", line)? as u8),
            "[ptr1+]" | "[p1+]" => {
                MoveLiteralIndirect1(narrow(immediate, 0xff, "literal", line)? as u8)
            }
            "[ptr0]" | "[p0]" => {
                MoveLiteralIndirect0(narrow(immediate, 0xff, "literal", line)? as u8)
            }
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
        "[ptr0]" | "idata0" => 0x00,
        "[ptr1+]" | "[ptr1]" | "idata1" => 0x01,
        "pcl" => 0x02,
        "status" => 0x03,
        "ptr0" | "p0" => 0x04,
        "timer0.count" => 0x05,
        "timer0.ctrl" => 0x06,
        "timer0.reload" => 0x07,
        "codec.period" => 0x08,
        "ptr1" | "p1" => 0x09,
        "pins.dir" => 0x0a,
        "pins" => 0x0b,
        "codec.ctrl" => 0x0c,
        "sys.ctrl" => 0x1c,
        "cmd.out" => 0x1d,
        "cmd.in" => 0x1e,
        "xchg" => 0x1f,
        _ if lower.starts_with('d')
            && lower[1..]
                .chars()
                .all(|character| character.is_ascii_digit()) =>
        {
            lower[1..]
                .parse::<u16>()
                .ok()
                .filter(|index| *index < 32)
                .map(|index| 0x20 + index)
                .ok_or_else(|| line_error(line, "data register must be d0..d31"))?
        }
        _ => eval(name, symbols, line)?,
    };
    Register::new(address).ok_or_else(|| line_error(line, "register address exceeds 0x1ff"))
}

fn canonical_bit_input(name: &str) -> Option<BitInput> {
    Some(match name {
        "codec.rx_bit" | "codec_rx" => BitInput::CodecRx,
        "pins.in0" | "pins.input0" | "io0" => BitInput::Io0,
        "pins.in1" | "pins.input1" | "io1" => BitInput::Io1,
        _ => return None,
    })
}

fn canonical_bit_output(name: &str) -> Option<BitIo> {
    Some(match name {
        "status.carry" | "carry" => BitIo::Carry,
        "codec.tx_bit" | "codec_bit" => BitIo::CodecBit,
        "pins.out0" | "pins.output0" | "io0" => BitIo::Io0,
        "pins.out1" | "pins.output1" | "io1" => BitIo::Io1,
        _ => return None,
    })
}

fn canonical_bit_source(name: &str) -> Option<BitIo> {
    Some(match name {
        "status.carry" | "carry" => BitIo::Carry,
        "codec.rx_bit" | "codec_rx" => BitIo::CodecBit,
        "pins.in0" | "pins.input0" | "io0" => BitIo::Io0,
        "pins.in1" | "pins.input1" | "io1" => BitIo::Io1,
        _ => return None,
    })
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
    for (name, value) in [
        ("[PTR0]", 0x00),
        ("[PTR1+]", 0x01),
        ("[PTR1]", 0x01),
        ("PCL", 0x02),
        ("STATUS", 0x03),
        ("PTR0", 0x04),
        ("TIMER0.COUNT", 0x05),
        ("TIMER0.CTRL", 0x06),
        ("TIMER0.RELOAD", 0x07),
        ("CODEC.PERIOD", 0x08),
        ("PTR1", 0x09),
        ("PINS.DIR", 0x0a),
        ("PINS", 0x0b),
        ("CODEC.CTRL", 0x0c),
        ("SYS.CTRL", 0x1c),
        ("CMD.OUT", 0x1d),
        ("CMD.IN", 0x1e),
        ("XCHG", 0x1f),
    ] {
        symbols.insert(name.into(), value);
    }
    for index in 0..32 {
        symbols.insert(format!("SFR_DATA_REG{index}"), 0x20 + index as u16);
        symbols.insert(format!("D{index}"), 0x20 + index as u16);
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

    #[test]
    fn canonical_v2_names_aliases_and_bit_endpoints_encode_exactly() {
        let source = r#"
            .alias delay_count = d1
            .const last_bit = 7
            clr delay_count
            mov pins.out1,xchg[last_bit]
            mov xchg[0],pins.in1
            mov c,pins.in1
            xor c,pins.in0
            mov a,[ptr1+]
            skip set,[ptr1][7]
        "#;
        let program = assemble_canonical(source).unwrap();
        assert_eq!(
            program.words,
            [0x0121, 0x00bf, 0x00f8, 0x001f, 0x001c, 0x0201, 0x5f01]
        );
    }
}
