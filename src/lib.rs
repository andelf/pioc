#![allow(non_camel_case_types)]

use core::fmt;

use regs::SFR;

pub mod regs;

macro_rules! define_bit_type {
    ($name:ident, $bits:expr, $inner:ty) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub struct $name($inner);

        impl From<$name> for $inner {
            fn from(x: $name) -> $inner {
                x.0
            }
        }

        impl From<$inner> for $name {
            fn from(x: $inner) -> $name {
                $name((x & ((1 << $bits) - 1)) as $inner)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{:?}_u{}", self.0, $bits)
            }
        }
    };
}

define_bit_type!(u2, 2, u8);
define_bit_type!(u3, 3, u8);
define_bit_type!(u7, 7, u8);
define_bit_type!(u9, 9, u16);
define_bit_type!(u10, 10, u16);
define_bit_type!(u12, 12, u16);

/// d
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dest {
    A,
    F,
}

impl From<bool> for Dest {
    fn from(value: bool) -> Self {
        match value {
            false => Dest::A,
            true => Dest::F,
        }
    }
}
impl From<Dest> for bool {
    fn from(value: Dest) -> bool {
        match value {
            Dest::A => false,
            Dest::F => true,
        }
    }
}

impl fmt::Display for Dest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Dest::A => write!(f, "A"),
            Dest::F => write!(f, "F"),
        }
    }
}

/// 4 Bit Output Sink
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitOut(u2);

impl fmt::Display for BitOut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 .0 {
            0 => write!(f, "Status[FLAG_C]"),
            1 => write!(f, "BitCycle[TX_O0]"),
            2 => write!(f, "PortIO[OUT0]"),
            3 => write!(f, "PortIO[OUT1]"),
            _ => unreachable!(),
        }
    }
}

/// 4 Bit Input Source
///
/// - BIO_FLAG_C   SB_FLAG_C
/// - BI_BIT_RX_I0 SB_BIT_RX_I0
/// - BI_PORT_IN0  SB_PORT_IN0
/// - BI_PORT_IN1  SB_PORT_IN1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitIn(u2);

impl fmt::Display for BitIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 .0 {
            0 => write!(f, "Status[FLAG_C]"),
            1 => write!(f, "BitCycle[RX_I0]"),
            2 => write!(f, "PortIO[IN0]"),
            3 => write!(f, "PortIO[IN1]"),
            _ => unreachable!(),
        }
    }
}

/// 4 Bit Input Source for BCTC
///
/// - BI_C_XOR_IN0
/// - BI_BIT_RX_I0
/// - BI_PORT_IN0
/// - BI_PORT_IN1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitInC(u2);

impl fmt::Display for BitInC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 .0 {
            0 => write!(f, "XOR_IN0"),
            1 => write!(f, "BitCycle[RX_I0]"),
            2 => write!(f, "PortIO[IN0]"),
            3 => write!(f, "PortIO[IN1]"),
            _ => unreachable!(),
        }
    }
}

/*
WB_DATA_SW_MR_0     EQU   0
WB_BIT_CYC_TAIL_1   EQU   1
WB_PORT_I0_FALL     EQU   2
WB_PORT_I0_RISE     EQU   3
WB_DATA_MW_SR_1     EQU   4
WB_PORT_XOR1_1      EQU   5
WB_PORT_XOR0_0      EQU   6
WB_PORT_XOR0_1      EQU   7
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitBit(u3);

impl fmt::Display for WaitBit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 .0 {
            0 => write!(f, "WB_DATA_SW_MR_0"),
            1 => write!(f, "WB_BIT_CYC_TAIL_1"),
            2 => write!(f, "WB_PORT_I0_FALL"),
            3 => write!(f, "WB_PORT_I0_RISE"),
            4 => write!(f, "WB_DATA_MW_SR_1"),
            5 => write!(f, "WB_PORT_XOR1_1"),
            6 => write!(f, "WB_PORT_XOR0_0"),
            7 => write!(f, "WB_PORT_XOR0_1"),
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Label<T>(T);

impl<T: fmt::Display> fmt::Display for Label<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ".L{}", self.0)
    }
}

/// u8 or u9
/// SFR 或 RAM 寄存器
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reg<T>(T);

impl Reg<u9> {
    pub fn normalize(&self) -> Reg<u8> {
        Reg(self.0 .0 as u8)
    }
}

impl fmt::Display for Reg<u8> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_sfr())
    }
}
impl fmt::Display for Reg<u9> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.normalize().to_sfr())
    }
}

impl Reg<u8> {
    pub fn to_sfr(&self) -> SFR {
        SFR::from_u8(self.0)
    }
}

/// OpCode of RISC8B, eMCU
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    /// NOP
    Nop,
    /// CLRWDT
    ClearWatchDog,
    /// SLEEP
    /// SLEEPX k2
    Sleep(u2),
    /*
    WB_DATA_SW_MR_0     EQU   0
    WB_BIT_CYC_TAIL_1   EQU   1
    WB_PORT_I0_FALL     EQU   2
    WB_PORT_I0_RISE     EQU   3
    WB_DATA_MW_SR_1     EQU   4
    WB_PORT_XOR1_1      EQU   5
    WB_PORT_XOR0_0      EQU   6
    WB_PORT_XOR0_1      EQU   7
     */
    /// WAITB b
    WaitB(WaitBit),
    // no WAITRD, WAITWR, WAITSPI
    // no idea of how to use  WRCODE, EXEC
    /// RDCODE k2
    // ROM_CODE(k2)->{SFR,A}
    ReadCode(u2),

    /// PUSHAS
    ///
    /// Push all
    PushA,
    /// POPAS
    ///
    /// Pop from TOS
    PopA,

    /// PUSHA2
    PushIndirAddr2,
    /// POPA2
    PopIndirAddr2,

    /// RET
    // 00000000 001100xx
    Return,
    /// RETZ        1->Z
    // 00000000 001101xx
    ReturnOk,
    /// RETIE - return interrupt
    // 00000000 001110xx
    ReturnInt,
    /// RETL k
    // 00100000 kkkkkkkk
    ReturnImm(u8),
    /// RETLN k
    // 0->Z
    // 00100001 kkkkkkkk
    ReturnErrImm(u8),

    /// CLRA
    // 00000000 000001xx
    ClearA,
    /// CLR f
    // 00000001 ffffffff
    Clear(Reg<u8>),

    /// MOVA F      A -> F
    /// Move A to F
    // 0001000F FFFFFFFF
    MoveA(Reg<u9>),

    /// MOV F       F -> f (Z)
    /// MOV F,A     F -> A (Z)
    // 000d001 FFFFFFFFF
    Move(Reg<u9>, Dest),

    /// INC f       f+1 -> f (Z)
    /// INC f,A     f+1 -> A (Z)
    // 000d0100 ffffffff
    Inc(Reg<u8>, Dest),
    // 000d0101 ffffffff
    Dec(Reg<u8>, Dest),

    /// INCSZ f,d
    // 000d0110
    IncAndSkipIfZero(Reg<u8>, Dest),
    /// DECSZ f,d
    // 000d0111
    DecAndSkipIfZero(Reg<u8>, Dest),

    /// SWAP f,d
    // 000d1000
    SwapHalfBytes(Reg<u8>, Dest),

    /// AND f,d
    // A & f -> A (Z)
    // 000d1001
    And(Reg<u8>, Dest),
    /// IOR f,d
    // 000d1010
    Or(Reg<u8>, Dest),
    // 000d1011
    Xor(Reg<u8>, Dest),
    // 000d1100
    Add(Reg<u8>, Dest),
    // SUB f,d
    // f-A->d
    // 000d1101
    Sub(Reg<u8>, Dest),

    /// RCL f,d
    // 000d1110
    RotateLeftWithCarry(Reg<u8>, Dest),
    /// RCR f,d
    // 000d1111
    RotateRightWithCarry(Reg<u8>, Dest),

    /// MOVIP k9
    // k9->{INDIR_RAM_PAGE, INDIR_RAM SFR_INDIR_ADDR}
    // 0010001k kkkkkkkk
    MoveImmToIndirAddr1(u9),
    /// MOVIA k10
    // k10→SFR_INDIR_ADDR2
    // 001001kk kkkkkkkk
    MoveImmToIndirAddr2(u10),

    // Fast moves
    // Fast1 SFR_PORT_DIR
    // Fast2 SFR_PORT_IO
    // P1 @SFR_INDIR_ADDR
    // P2 @SFR_INDIR_ADDR2
    /// MOVA1F
    /// SFR_PORT_DIR
    // 00100011 kkkkkkkk
    MoveImmToPortDir(u8),
    /// MOVA2F
    /// SFR_PORT_IO
    // 00100101 kkkkkkkk
    MoveImmToPortIo(u8),
    /// MOVA1P
    // 00100111 kkkkkkkk
    MoveImmToP1(u8),
    /// MOVA2P
    // 00100110 kkkkkkkk
    MoveImmToP2(u8),

    /// MOVL k
    /// k -> A
    // 00101000 kkkkkkkk
    MoveImm(u8),
    /// ADDL
    // 00101001 kkkkkkkk
    AndImm(u8),
    /// ORL
    // 00101010 kkkkkkkk
    OrImm(u8),
    /// XORL
    // 00101011 kkkkkkkk
    XorImm(u8),
    /// ADDL
    // 00101100 kkkkkkkk
    AddImm(u8),
    /// SUBL
    // 00101101 kkkkkkkk
    SubImm(u8),
    /// CMPLN
    // 00101110 kkkkkkkk
    CompareImmNegate(u8),
    /// CMPL
    // 00101111 kkkkkkkk
    CompareImm(u8),

    // bit op
    /// BC
    // 01000bbb ffffffff
    BitClear(Reg<u8>, u3),
    /// BS
    // 01001bbb, ffffffff
    BitSet(Reg<u8>, u3),

    /// BTSC
    // 01010bbb, ffffffff
    BitTestSkipIfClear(Reg<u8>, u3),
    /// BTSS
    // 01011bbb ffffffff
    BitTestSkipIfSet(Reg<u8>, u3),

    /// BCTC
    // BI_C_XOR_IN0        EQU   0
    // BI_BIT_RX_I0        EQU   1
    // BI_PORT_IN0         EQU   2
    // BI_PORT_IN1         EQU   3
    BitToC(BitInC),

    // BP1F, BP2F, BG1F, BG2F
    // F1: SFR_INDIR_ADDR
    // F2: SFR_DATA_EXCH
    /// BP1F
    // 1#bf[b]->bit[a]
    // 00000000 100aabbb
    // SFR_INDIR_ADDR
    BitOut1(BitOut, u3),
    /// BP2F
    // 2#bf[b]->bit[a]
    // 00000000 101aabbb
    // SFR_DATA_EXCH
    BitOut2(BitOut, u3),
    /// BG1F
    // bit[a]->1#bf[b]
    // 00000000 110aabbb
    // SFR_INDIR_ADDR
    BitIn1(BitIn, u3),
    /// BG2F
    // bit[a]->2#bf[b]
    // 00000000 111aabbb
    // SFR_DATA_EXCH
    BitIn2(BitIn, u3),

    /// JMP
    // 0110kkkk kkkkkkkk
    Jump(Label<u12>),
    /// CALL
    Call(Label<u12>),

    /// JNZ
    JumpIfNotZero(Label<u10>),
    /// JZ
    JumpIfZero(Label<u10>),
    /// JNC
    /// Jump if C==0
    JumpIfNotCarry(Label<u10>),
    /// JC
    /// Jump if C==1
    JumpIfCarry(Label<u10>),
    /// CMPZ K7,k
    // JEQ
    /// jump to u8 if u7==A
    // 1KKKKKKK kkkkkkkk
    JumpIfEqual(u7, Label<u8>),

    Unknown(u16),
}

impl OpCode {
    pub fn to_bytes(&self) -> [u8; 2] {
        match self {
            OpCode::Nop => [0, 0],
            OpCode::ClearWatchDog => [0x00, 0b00001000],
            OpCode::Sleep(k2) => [0x00, 0b00001100 | (k2.0 & 0b11)],
            OpCode::WaitB(b) => [0x00, 0b00010000 | (b.0 .0)],
            OpCode::PushA => [0x00, 0b00100000],
            OpCode::PopA => [0x00, 0b00100100],
            _ => todo!(),
        }
    }

    pub fn to_wch_risc8b_asm(&self) -> String {
        match self {
            OpCode::Nop => format!("NOP"),
            OpCode::ClearWatchDog => format!("CLRWDT"),
            OpCode::Sleep(k2) => format!("SLEEPX {}", k2),
            OpCode::WaitB(b) => format!("WAITB {}", b),
            OpCode::PushA => format!("PUSHA"),
            OpCode::PopA => format!("POPA"),
            OpCode::ReadCode(k2) => format!("RCODE {}", k2),
            OpCode::Return => format!("RET"),
            OpCode::ReturnOk => format!("RETZ"),
            OpCode::ReturnInt => format!("RETIE"),
            OpCode::ReturnImm(k) => format!("RETL {}", k),
            OpCode::ReturnErrImm(k) => format!("RETLN {}", k),
            OpCode::ClearA => format!("CLRA"),
            OpCode::Clear(f) => format!("CLR {}\t; 0x00->{0}, 1->Z", f),
            OpCode::MoveA(f) => format!("MOVA {}\t; A->{0}", f),
            OpCode::Move(f, d) => format!("MOV {}, {}\t; {0}->{1}", f, d),
            OpCode::Inc(f, d) => format!("INC {}, {}\t; {0}+1->{1}", f, d),
            OpCode::Dec(f, d) => format!("DEC {}, {}\t; {0}-1->{1}", f, d),
            OpCode::IncAndSkipIfZero(f, d) => {
                format!("INCSZ {}, {}\t; {0}+1->{1}, skip if Z", f, d)
            }
            OpCode::DecAndSkipIfZero(f, d) => {
                format!("DECSZ {}, {}\t; {0}-1->{1}, skip if Z", f, d)
            }
            OpCode::SwapHalfBytes(f, d) => {
                format!("SWAP {}, {}\t; {0}[3:0]<=>{0}[7:4] -> {1}", f, d)
            }
            OpCode::And(f, d) => format!("AND {}, {}\t; {0}&A->{1}", f, d),
            OpCode::Or(f, d) => format!("IOR {}, {}\t; {0}|A->{1}", f, d),
            OpCode::Xor(f, d) => format!("XOR {}, {}\t; {0}^A->{1}", f, d),
            OpCode::Add(f, d) => format!("ADD {}, {}\t; {0}+A->{1}", f, d),
            OpCode::Sub(f, d) => format!("SUB {}, {}\t; {0}-A->{1}", f, d),
            OpCode::RotateLeftWithCarry(f, d) => {
                format!("RCL {}, {}\t; {{{0},C}}<<1->{1},{0}[7]->C", f, d)
            }
            OpCode::RotateRightWithCarry(f, d) => format!("RCR {}, {}", f, d),

            OpCode::MoveImmToIndirAddr1(j) => format!("MOVIP {}\t; {0}->SFR_INDIR_ADDR", j),
            OpCode::MoveImmToIndirAddr2(k) => format!("MOVIA {}\t; {0}->SFR_INDIR_ADDR2", k),
            OpCode::MoveImmToPortDir(k) => format!("MOVA1F {}\t; {0}->SFR_PORT_DIR", k),
            OpCode::MoveImmToPortIo(k) => format!("MOVA2F {}\t; {0}->SFR_PORT_IO", k),
            OpCode::MoveImmToP2(k) => format!("MOVA2P {}\t; {0}->@SFR_INDIR_ADDR2", k),
            OpCode::MoveImmToP1(k) => format!("MOVA1P {}\t; {0}->@SFR_INDIR_ADDR", k),

            OpCode::MoveImm(k) => format!("MOVL {}\t; {0}->A", k),
            OpCode::AndImm(k) => format!("ANDL {}\t; {0}&A->A", k),
            OpCode::OrImm(k) => format!("IORL {}\t; {0}|A->A", k),
            OpCode::XorImm(k) => format!("XORL {}\t; {0}^A->A", k),
            OpCode::AddImm(k) => format!("ADDL {}\t; {0}+A->A", k),
            OpCode::SubImm(k) => format!("SUBL {}\t; A-{0}->A", k),
            OpCode::CompareImmNegate(k) => format!("CMPLN {}\t; {0}+A -> Z,C", k),
            OpCode::CompareImm(k) => format!("CMPL {}\t; {0}-A -> Z,C", k),
            OpCode::BitClear(f, b) => format!("BC {}, {}\t; 0->{0}[{1}]", f, b),
            OpCode::BitSet(f, b) => format!("BS {}, {}\t; 1->{0}[{1}]", f, b),
            OpCode::BitTestSkipIfClear(f, b) => format!("BTSC {}, {}\t; skip if {0}[{1}]==0", f, b),
            OpCode::BitTestSkipIfSet(f, b) => format!("BTSS {}, {}\t; skip if {0}[{1}]==1", f, b),
            OpCode::BitToC(a) => format!("BCTC {}\t; {0}->C", a),
            OpCode::BitOut1(a, b) => {
                format!("BP1F {}, {}\t; SFR_INDIR_ADDR[{1}]-> {0}", a, b)
            }
            OpCode::BitOut2(a, b) => {
                format!("BP2F {}, {}\t; SFR_DATA_EXCH[{1}]->{0}", a, b)
            }
            OpCode::BitIn1(a, b) => format!("BG1F {}, {}\t; {0}->SFR_INDIR_ADDR[{1}]", a, b),
            OpCode::BitIn2(a, b) => format!("BG2F {}, {}\t; {0}->SFR_DATA_EXCH[{1}]", a, b),

            // jumps
            OpCode::Jump(k12) => format!("JMP {}", k12),
            OpCode::Call(k12) => format!("CALL {}", k12),
            OpCode::JumpIfNotZero(k) => format!("JNZ {}", k),
            OpCode::JumpIfZero(k) => format!("JZ {}", k),
            OpCode::JumpIfNotCarry(k) => format!("JNC {}", k),
            OpCode::JumpIfCarry(k10) => format!("JC {}", k10),
            OpCode::JumpIfEqual(k7, label) => {
                format!("CMPZ {}, {}\t; {1}->PC[7:0] if A=={0}", k7, label)
            }
            OpCode::PushIndirAddr2 => format!("PUSHA2"),
            OpCode::PopIndirAddr2 => format!("POPA2"),
            OpCode::Unknown(_) => todo!(),
        }
    }

    pub fn from_word(word: u16) -> OpCode {
        let k = (word & 0xFF) as u8;
        match (word >> 8) as u8 {
            0x00 if k & 0b111111_00 == 0x00 => OpCode::Nop,
            0x00 if k & 0b111111_00 == 0b000010_00 => OpCode::ClearWatchDog,
            0x00 if k & 0b111111_00 == 0b00001100 => OpCode::Sleep(k.into()),
            0x00 if k & 0b111111_00 == 0b001000_00 => OpCode::PushA,
            0x00 if k & 0b111111_00 == 0b001001_00 => OpCode::PopA,
            0x00 if k & 0b111111_00 == 0b001010_00 => OpCode::PushIndirAddr2,
            0x00 if k & 0b111111_00 == 0b001011_00 => OpCode::PopIndirAddr2,

            0x00 if k & 0b111111_00 == 0b001100_00 => OpCode::Return,
            0x00 if k & 0b111111_00 == 0b001101_00 => OpCode::ReturnOk,
            0x00 if k & 0b111111_00 == 0b001110_00 => OpCode::ReturnInt,

            0x00 if k & 0b11111000 == 0b00010_000 => OpCode::WaitB(WaitBit(k.into())),
            0x00 if k & 0b111111_00 == 0b000001_00 => OpCode::ClearA,

            0x00 if k & 0b111111_00 == 0b000111_00 => OpCode::BitToC(BitInC(k.into())),
            0x00 if k & 0b111_00000 == 0b100_00_000 => {
                OpCode::BitOut1(BitOut((k >> 3).into()), k.into())
            }
            0x00 if k & 0b111_00000 == 0b101_00_000 => {
                OpCode::BitOut2(BitOut((k >> 3).into()), k.into())
            }
            0x00 if k & 0b111_00000 == 0b110_00_000 => {
                OpCode::BitIn1(BitIn((k >> 3).into()), k.into())
            }
            0x00 if k & 0b111_00000 == 0b111_00_000 => {
                OpCode::BitIn2(BitIn((k >> 3).into()), k.into())
            }
            0x00 if k & 0b111111_00 == 0b000110_00 => OpCode::ReadCode(k.into()),

            0x00 if k == 0b00010100 => todo!("WAITWR"),

            0b000000_1 => OpCode::Clear(Reg(k.into())),

            // immediate byte op
            0b00101000 => OpCode::MoveImm(k),
            0b00101001 => OpCode::AndImm(k),
            0b00101010 => OpCode::OrImm(k),
            0b00101011 => OpCode::XorImm(k),
            0b00101100 => OpCode::AddImm(k),
            0b00101101 => OpCode::SubImm(k),
            0b00101110 => OpCode::CompareImmNegate(k),
            0b00101111 => OpCode::CompareImm(k),

            0b00100000 => OpCode::ReturnImm(k),
            0b00100001 => OpCode::ReturnErrImm(k),

            // byte op
            0b00100011 => OpCode::MoveImmToPortDir(k),
            0b00100101 => OpCode::MoveImmToPortIo(k),
            0b00100110 => OpCode::MoveImmToP2(k),
            0b00100111 => OpCode::MoveImmToP2(k),

            x if x & 0b1111111_0 == 0b0010001_0 => OpCode::MoveImmToIndirAddr1(word.into()),
            x if x & 0b111111_00 == 0b001001_00 => OpCode::MoveImmToIndirAddr2(word.into()),

            x if x & 0b1111111_0 == 0b0001000_0 => OpCode::MoveA(Reg(word.into())),
            x if x & 0b111_0_111_0 == 0b000_0_001_0 => {
                OpCode::Move(Reg(word.into()), Dest::from(x & 0b000_1_000_0 != 0))
            }

            x if x & 0b111_0_1111 == 0b000_0_0100 => {
                OpCode::Inc(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }
            x if x & 0b111_0_1111 == 0b000_0_0101 => {
                OpCode::Dec(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }
            x if x & 0b111_0_1111 == 0b000_0_0110 => {
                OpCode::IncAndSkipIfZero(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }
            x if x & 0b111_0_1111 == 0b000_0_0111 => {
                OpCode::DecAndSkipIfZero(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }
            x if x & 0b111_0_1111 == 0b000_0_1000 => {
                OpCode::SwapHalfBytes(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }
            x if x & 0b111_0_1111 == 0b000_0_1001 => {
                OpCode::And(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }
            x if x & 0b111_0_1111 == 0b000_0_1010 => {
                OpCode::Or(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }
            x if x & 0b111_0_1111 == 0b000_0_1011 => {
                OpCode::Xor(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }
            x if x & 0b111_0_1111 == 0b000_0_1100 => {
                OpCode::Add(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }
            x if x & 0b111_0_1111 == 0b000_0_1101 => {
                OpCode::Sub(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }
            x if x & 0b111_0_1111 == 0b000_0_1110 => {
                OpCode::RotateLeftWithCarry(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }
            x if x & 0b111_0_1111 == 0b000_0_1111 => {
                OpCode::RotateRightWithCarry(Reg(k), Dest::from(x & 0b000_1_0000 != 0))
            }

            x if x & 0b1111_0000 == 0b0110_0000 => {
                let label: Label<u12> = Label(word.into());
                OpCode::Jump(label)
            }
            x if x & 0b1111_0000 == 0b0111_0000 => OpCode::Call(Label(word.into())),
            x if x & 0b111111_00 == 0b001100_00 => OpCode::JumpIfZero(Label(word.into())),
            x if x & 0b111111_00 == 0b001101_00 => OpCode::JumpIfNotZero(Label(word.into())),
            x if x & 0b111111_00 == 0b001110_00 => OpCode::JumpIfCarry(Label(word.into())),
            x if x & 0b111111_00 == 0b001111_00 => OpCode::JumpIfNotCarry(Label(word.into())),
            x if x & 0b1_0000000 == 0b1_0000000 => {
                OpCode::JumpIfEqual(u7::from(x), Label(u8::from(k)))
            }

            // bit op
            x if x & 0b11111_000 == 0b01000_000 => OpCode::BitClear(Reg(k.into()), x.into()),
            x if x & 0b11111_000 == 0b01001_000 => OpCode::BitSet(Reg(k.into()), x.into()),
            x if x & 0b11111_000 == 0b01010_000 => {
                OpCode::BitTestSkipIfClear(Reg(k.into()), x.into())
            }
            x if x & 0b11111_000 == 0b01011_000 => {
                OpCode::BitTestSkipIfSet(Reg(k.into()), x.into())
            }

            _ => {
                println!("Unknown opcode: {:04X} {:016b}", word, word);
                todo!()
            }
        }
    }
}

/*
MOV src, dst

CLR A
CLR d0


*/
