//! Legacy WCH and canonical PIOC assembly front-end.

mod assemble;
mod print;

use pioc_core::{DecodeTarget, DecodedInstruction, Instruction};
use thiserror::Error;

pub use assemble::{
    assemble_canonical, assemble_file, assemble_file_with_dependencies, assemble_file_with_syntax,
    assemble_legacy, translate_legacy, translate_legacy_file,
};
pub use print::{format_decoded, format_instruction, format_register};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    Legacy,
    Canonical,
}

impl Syntax {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Canonical => "canonical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub words: Vec<u16>,
}

impl Program {
    pub fn from_le_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() % 2 != 0 {
            return Err(ParseError::OddBinaryLength(bytes.len()));
        }
        Ok(Self {
            words: bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect(),
        })
    }

    pub fn decode(&self, target: DecodeTarget) -> Vec<DecodedInstruction> {
        self.words
            .iter()
            .copied()
            .map(|word| Instruction::decode(word, target))
            .collect()
    }

    pub fn to_le_bytes(&self) -> Vec<u8> {
        self.words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("PIOC binary length {0} is odd; instructions are 16-bit words")]
    OddBinaryLength(usize),
    #[error("assembly parser is not implemented for line {line}: {message}")]
    Assembly { line: usize, message: String },
    #[error("{path}:{line}: {message}")]
    Source {
        path: String,
        line: usize,
        message: String,
    },
    #[error("failed to read {path}: {message}")]
    Io { path: String, message: String },
}
