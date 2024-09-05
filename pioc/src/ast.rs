use crate::OpCode;

pub struct Program {
    pub code: Vec<u16>,
}

pub enum Statement {
    Include(String),
    Org(u16),
    RawWord(u16),
    Define(String, String),
    Label(String),
    Instruction(OpCode),
    End,
    RawAssembly(String), // unparsed
}
