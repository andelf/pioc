use std::fmt;

/// Special Function Registers, 0x00 to 0x3F
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SFR {
    ProgramCounter = 0x02,
    /// Status Register (SR)
    /// SR.TR (SB_EN_TOUT_RST): Timeout Reset Enable
    /// SR.SU (SB_STACK_USED): Stack Used
    /// SR.GY (SB_GP_BIT_Y): General Purpose Bit Y
    /// SR.Z (SB_FLAG_Z): Zero Flag
    /// SR.GX (SB_GP_BIT_X): General Purpose Bit X
    /// SR.C (SB_FLAG_C): Carry Flag
    Status = 0x03,

    IndirPort1 = 0x00,
    IndirPort2 = 0x01,
    /// Indirect Address Register 1, aka. F1
    IndirAddr1 = 0x04,
    /// Indirect Address Register 2, auto-incremented after each access
    IndirAddr2 = 0x09,

    TimerCount = 0x05,
    /// Timer Control Register (TMRCTL)
    /// TMRCTL.L1E (SB_EN_LEVEL1): Enable Level 1
    /// TMRCTL.L0E (SB_EN_LEVEL0): Enable Level 0
    /// TMRCTL.T0E (SB_TMR0_ENABLE): Timer 0 Enable
    /// TMRCTL.T0OE (SB_TMR0_OUT_EN): Timer 0 Output Enable
    /// TMRCTL.T0M (SB_TMR0_MODE): Timer 0 Mode
    /// TMRCTL.T0F2 (SB_TMR0_FREQ2): Timer 0 Frequency Bit 2
    /// TMRCTL.T0F1 (SB_TMR0_FREQ1): Timer 0 Frequency Bit 1
    /// TMRCTL.T0F0 (SB_TMR0_FREQ0): Timer 0 Frequency Bit 0
    TimerCtrl = 0x06,
    TimerInit = 0x07,
    /// Encoding Bit Period Register, readable and writable by Host MCU
    /// Bit Cycle Register (BITCYC)
    /// BITCYC.TXO (SB_BIT_TX_O0): Bit Transmit Output 0
    /// BITCYC.C6 (SB_BIT_CYCLE_6): Bit Cycle 6
    /// BITCYC.C5 (SB_BIT_CYCLE_5): Bit Cycle 5
    /// BITCYC.C4 (SB_BIT_CYCLE_4): Bit Cycle 4
    /// BITCYC.C3 (SB_BIT_CYCLE_3): Bit Cycle 3
    /// BITCYC.C2 (SB_BIT_CYCLE_2): Bit Cycle 2
    /// BITCYC.C1 (SB_BIT_CYCLE_1): Bit Cycle 1
    /// BITCYC.C0 (SB_BIT_CYCLE_0): Bit Cycle 0
    BitCycle = 0x08,
    /// Port Direction Register (PDIR)
    /// PDIR.M3 (SB_PORT_MOD3): Port Mode 3
    /// PDIR.M2 (SB_PORT_MOD2): Port Mode 2
    /// PDIR.M1 (SB_PORT_MOD1): Port Mode 1
    /// PDIR.M0 (SB_PORT_MOD0): Port Mode 0
    /// PDIR.PU1 (SB_PORT_PU1): Port Pull-Up 1
    /// PDIR.PU0 (SB_PORT_PU0): Port Pull-Up 0
    /// PDIR.D1 (SB_PORT_DIR1): Port Direction 1
    /// PDIR.D0 (SB_PORT_DIR0): Port Direction 0
    PortDir = 0x0A,
    /// Port I/O Register (PIO)
    /// PIO.INXOR (SB_PORT_IN_XOR): Port Input XOR
    /// PIO.RXI (SB_BIT_RX_I0): Bit Receive Input 0
    /// PIO.IN1 (SB_PORT_IN1): Port Input 1
    /// PIO.IN0 (SB_PORT_IN0): Port Input 0
    /// PIO.XOR1 (SB_PORT_XOR1): Port XOR 1
    /// PIO.XOR0 (SB_PORT_XOR0): Port XOR 0
    /// PIO.OUT1 (SB_PORT_OUT1): Port Output 1
    /// PIO.OUT0 (SB_PORT_OUT0): Port Output 0
    PortIO = 0x0B,

    /// Bit Configuration Register (BITCFG)
    /// BITCFG.TXE (SB_BIT_TX_EN): Bit Transmit Enable
    /// BITCFG.CMOD (SB_BIT_CODE_MOD): Bit Code Mode
    /// BITCFG.EDGE (SB_PORT_IN_EDGE): Port Input Edge
    /// BITCFG.TAIL (SB_BIT_CYC_TAIL): Bit Cycle Tail
    /// BITCFG.CC6 (SB_BIT_CYC_CNT6): Bit Cycle Count 6
    /// BITCFG.CC5 (SB_BIT_CYC_CNT5): Bit Cycle Count 5
    /// BITCFG.CC4 (SB_BIT_CYC_CNT4): Bit Cycle Count 4
    /// BITCFG.CC3 (SB_BIT_CYC_CNT3): Bit Cycle Count 3
    BitConfig = 0x0C,
    /// System Configuration Register (SYSCFG)
    /// SYSCFG.INTQ (SB_INT_REQ): Interrupt Request
    /// SYSCFG.DSMR (SB_DATA_SW_MR): Data Switch Master to RAM
    /// SYSCFG.DMSR (SB_DATA_MW_SR): Data Master to Switch RAM
    /// SYSCFG.MCB4 (SB_MST_CFG_B4): Master Config Bit 4
    /// SYSCFG.MIO1 (SB_MST_IO_EN1): Master I/O Enable 1
    /// SYSCFG.MIO0 (SB_MST_IO_EN0): Master I/O Enable 0
    /// SYSCFG.MRST (SB_MST_RESET): Master Reset
    /// SYSCFG.MCLKG (SB_MST_CLK_GATE): Master Clock Gate
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

    Error = 0xFF,
}

/// Display as the same as Debug
impl fmt::Display for SFR {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // write!(f, "SFR::{:?}", self)
        write!(f, "${}", self.to_name())
    }
}

impl SFR {
    pub fn from_u8(val: u8) -> Self {
        if val <= 0x3f {
            unsafe { std::mem::transmute(val) }
        } else {
            SFR::Error
        }
    }

    pub fn to_name(&self) -> &'static str {
        match self {
            SFR::ProgramCounter => "PC",
            SFR::Status => "SR",
            SFR::IndirPort1 => "IP1",
            SFR::IndirPort2 => "IP2",
            SFR::IndirAddr1 => "IA1",
            SFR::IndirAddr2 => "IA2",
            SFR::TimerCount => "TMRCNT",
            SFR::TimerCtrl => "TMRCTL",
            SFR::TimerInit => "TMRINIT",
            SFR::BitCycle => "BITCYC",
            SFR::PortDir => "PDIR",
            SFR::PortIO => "PIO",
            SFR::BitConfig => "BITCFG",
            SFR::SysConf => "SYSCFG",
            SFR::CtrlRead => "CTLRD",
            SFR::CtrlWrite => "CTLWR",
            SFR::DataExch => "DEXCH",
            SFR::Data0 => "D0",
            SFR::Data1 => "D1",
            SFR::Data2 => "D2",
            SFR::Data3 => "D3",
            SFR::Data4 => "D4",
            SFR::Data5 => "D5",
            SFR::Data6 => "D6",
            SFR::Data7 => "D7",
            SFR::Data8 => "D8",
            SFR::Data9 => "D9",
            SFR::Data10 => "D10",
            SFR::Data11 => "D11",
            SFR::Data12 => "D12",
            SFR::Data13 => "D13",
            SFR::Data14 => "D14",
            SFR::Data15 => "D15",
            SFR::Data16 => "D16",
            SFR::Data17 => "D17",
            SFR::Data18 => "D18",
            SFR::Data19 => "D19",
            SFR::Data20 => "D20",
            SFR::Data21 => "D21",
            SFR::Data22 => "D22",
            SFR::Data23 => "D23",
            SFR::Data24 => "D24",
            SFR::Data25 => "D25",
            SFR::Data26 => "D26",
            SFR::Data27 => "D27",
            SFR::Data28 => "D28",
            SFR::Data29 => "D29",
            SFR::Data30 => "D30",
            SFR::Data31 => "D31",
            SFR::Error => "ERR",
        }
    }

    // ... (保留之前的 to_name 函数)

    pub fn parse(s: &str) -> Option<SFR> {
        match s.to_uppercase().as_str() {
            "PC" => Some(SFR::ProgramCounter),
            "SR" => Some(SFR::Status),
            "IP1" => Some(SFR::IndirPort1),
            "IP2" => Some(SFR::IndirPort2),
            "IA1" => Some(SFR::IndirAddr1),
            "IA2" => Some(SFR::IndirAddr2),
            "TMRCNT" => Some(SFR::TimerCount),
            "TMRCTL" => Some(SFR::TimerCtrl),
            "TMRINIT" => Some(SFR::TimerInit),
            "BITCYC" => Some(SFR::BitCycle),
            "PDIR" => Some(SFR::PortDir),
            "PIO" => Some(SFR::PortIO),
            "BITCFG" => Some(SFR::BitConfig),
            "SYSCFG" => Some(SFR::SysConf),
            "CTLRD" => Some(SFR::CtrlRead),
            "CTLWR" => Some(SFR::CtrlWrite),
            "DEXCH" => Some(SFR::DataExch),
            s if s.starts_with('D') => {
                if let Ok(num) = s[1..].parse::<u8>() {
                    match num {
                        0..=31 => Some(unsafe { std::mem::transmute(0x20 + num) }),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            "ERR" => Some(SFR::Error),
            _ => None,
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
