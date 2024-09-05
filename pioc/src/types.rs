use std::fmt;

use crate::regs::SFR;

macro_rules! define_bit_type {
    ($name:ident, $bits:expr, $inner:ty) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub struct $name(pub $inner);

        impl $name {
            pub const fn new(value: $inner) -> Option<Self> {
                if value & ((1 << $bits) - 1) == value {
                    Some(Self(value))
                } else {
                    None
                }
            }
        }

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

define_bit_type!(U2, 2, u8);
define_bit_type!(U3, 3, u8);
define_bit_type!(U7, 7, u8);
define_bit_type!(U9, 9, u16);
define_bit_type!(U10, 10, u16);
define_bit_type!(U12, 12, u16);
// define_bit_type!(U8, 8, u8);
// define_bit_type!(U16, 16, u16);

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
pub struct BitOut(pub U2);

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
pub struct BitIn(pub U2);

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
pub struct BitInC(pub U2);

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
pub struct WaitBit(pub U3);

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
pub struct Label<T>(pub T);

impl<T: fmt::Display> fmt::Display for Label<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ".L{}", self.0)
    }
}

/// u8 or U9
/// SFR 或 RAM 寄存器
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reg<T>(pub T);

impl Reg<U9> {
    pub fn normalize(&self) -> Reg<u8> {
        Reg(self.0 .0 as u8)
    }
}

impl fmt::Display for Reg<u8> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_sfr())
    }
}
impl fmt::Display for Reg<U9> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.normalize().to_sfr())
    }
}

impl Reg<u8> {
    pub fn to_sfr(&self) -> SFR {
        SFR::from_u8(self.0)
    }
}
