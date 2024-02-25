use std::fmt;

/// Special Function Registers, 0x00 to 0x3F
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SFR {
    ProgramCounter = 0x02,
    Status = 0x03,

    IndirPort1 = 0x00,
    IndirPort2 = 0x01,
    /// Indirect Address Register 1, aka. F1
    IndirAddr1 = 0x04,
    /// Indirect Address Register 2, auto-incremented after each access
    IndirAddr2 = 0x09,

    TimerCount = 0x05,
    TimerCtrl = 0x06,
    TimerInit = 0x07,
    /// Encoding Bit Period Register, readable and writable by Host MCU
    BitCycle = 0x08,
    PortDir = 0x0A,
    PortIO = 0x0B,

    BitConfig = 0x0C,
    SysConf = 0x1C,
    CtrlRead = 0x1D,
    CtrlWrite = 0x1E,

    /// Data Exchange Register, aka. F2
    DataExch = 0x1F,

    Data0 = 0x20,
    Data1 = 0x21,
    Data2 = 0x22,
    Data3 = 0x23,
    Data4 = 0x24,
    Data5 = 0x25,
    Data6 = 0x26,
    Data7 = 0x27,
    Data8 = 0x28,
    Data9 = 0x29,
    Data10 = 0x2A,
    Data11 = 0x2B,
    Data12 = 0x2C,
    Data13 = 0x2D,
    Data14 = 0x2E,
    Data15 = 0x2F,
    Data16 = 0x30,
    Data17 = 0x31,
    Data18 = 0x32,
    Data19 = 0x33,
    Data20 = 0x34,
    Data21 = 0x35,
    Data22 = 0x36,
    Data23 = 0x37,
    Data24 = 0x38,
    Data25 = 0x39,
    Data26 = 0x3A,
    Data27 = 0x3B,
    Data28 = 0x3C,
    Data29 = 0x3D,
    Data30 = 0x3E,
    Data31 = 0x3F,

    ERROR = 0xFF,
}

/// Display as the same as Debug
impl fmt::Display for SFR {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SFR::{:?}", self)
    }
}

impl SFR {
    pub fn from_u8(val: u8) -> Self {
        if val <= 0x3f {
            unsafe { std::mem::transmute(val) }
        } else {
            SFR::ERROR
        }
    }

    pub fn to_wch_risc8b_name(&self) -> &'static str {
        match *self as u8 {
            0x00 => "SFR_INDIR_PORT",
            0x01 => "SFR_INDIR_PORT2",
            0x02 => "SFR_PRG_COUNT",
            0x03 => "SFR_STATUS_REG",
            0x04 => "SFR_INDIR_ADDR",
            0x05 => "SFR_TMR0_COUNT",
            0x06 => "SFR_TIMER_CTRL",
            0x07 => "SFR_TMR0_INIT",
            0x08 => "SFR_BIT_CYCLE",
            0x09 => "SFR_INDIR_ADDR2",
            0x0A => "SFR_PORT_DIR",
            0x0B => "SFR_PORT_IO",
            0x0C => "SFR_BIT_CONFIG",
            0x1C => "SFR_SYS_CFG",
            0x1D => "SFR_CTRL_RD",
            0x1E => "SFR_CTRL_WR",
            0x1F => "SFR_DATA_EXCH",
            0x20 => "SFR_DATA_REG0",
            0x21 => "SFR_DATA_REG1",
            0x22 => "SFR_DATA_REG2",
            0x23 => "SFR_DATA_REG3",
            0x24 => "SFR_DATA_REG4",
            0x25 => "SFR_DATA_REG5",
            0x26 => "SFR_DATA_REG6",
            0x27 => "SFR_DATA_REG7",
            0x28 => "SFR_DATA_REG8",
            0x29 => "SFR_DATA_REG9",
            0x2A => "SFR_DATA_REG10",
            0x2B => "SFR_DATA_REG11",
            0x2C => "SFR_DATA_REG12",
            0x2D => "SFR_DATA_REG13",
            0x2E => "SFR_DATA_REG14",
            0x2F => "SFR_DATA_REG15",
            0x30 => "SFR_DATA_REG16",
            0x31 => "SFR_DATA_REG17",
            0x32 => "SFR_DATA_REG18",
            0x33 => "SFR_DATA_REG19",
            0x34 => "SFR_DATA_REG20",
            0x35 => "SFR_DATA_REG21",
            0x36 => "SFR_DATA_REG22",
            0x37 => "SFR_DATA_REG23",
            0x38 => "SFR_DATA_REG24",
            0x39 => "SFR_DATA_REG25",
            0x3A => "SFR_DATA_REG26",
            0x3B => "SFR_DATA_REG27",
            0x3C => "SFR_DATA_REG28",
            0x3D => "SFR_DATA_REG29",
            0x3E => "SFR_DATA_REG30",
            0x3F => "SFR_DATA_REG31",
            _ => "!!SFR_ERROR!!",
        }
    }
}
