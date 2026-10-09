use core::fmt;

/// A directly addressable RISC8B register.
///
/// Extended `MOV`/`MOVA` instructions carry nine address bits. Most PIOC SFRs
/// are in the low 0x40 addresses, but preserving all nine bits is important for
/// a lossless assembler and generic RISC8B tooling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Register(u16);

impl Register {
    pub const MAX: u16 = 0x1ff;

    pub const fn new(address: u16) -> Option<Self> {
        if address <= Self::MAX {
            Some(Self(address))
        } else {
            None
        }
    }

    pub const fn address(self) -> u16 {
        self.0
    }

    pub const fn low(address: u8) -> Self {
        Self(address as u16)
    }
}

impl From<Sfr> for Register {
    fn from(value: Sfr) -> Self {
        Self(value as u8 as u16)
    }
}

/// Implemented CH32X035 PIOC special-function registers.
///
/// Reserved addresses 0x0d..=0x1b deliberately have no enum variant. Converting
/// an arbitrary byte is fallible, avoiding the invalid-enum `transmute` used by
/// the original prototype.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sfr {
    IndirectData0 = 0x00,
    IndirectData1 = 0x01,
    ProgramCounterLow = 0x02,
    Status = 0x03,
    IndirectAddress0 = 0x04,
    Timer0Count = 0x05,
    Timer0Control = 0x06,
    Timer0Reload = 0x07,
    CodecPeriod = 0x08,
    IndirectAddress1 = 0x09,
    PinsDirection = 0x0a,
    Pins = 0x0b,
    CodecControl = 0x0c,
    SystemControl = 0x1c,
    CommandOut = 0x1d,
    CommandIn = 0x1e,
    Exchange = 0x1f,
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
    Data10 = 0x2a,
    Data11 = 0x2b,
    Data12 = 0x2c,
    Data13 = 0x2d,
    Data14 = 0x2e,
    Data15 = 0x2f,
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
    Data26 = 0x3a,
    Data27 = 0x3b,
    Data28 = 0x3c,
    Data29 = 0x3d,
    Data30 = 0x3e,
    Data31 = 0x3f,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SfrError(pub u8);

impl fmt::Display for SfrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:02x} is not an implemented PIOC SFR", self.0)
    }
}

impl TryFrom<u8> for Sfr {
    type Error = SfrError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let sfr = match value {
            0x00 => Self::IndirectData0,
            0x01 => Self::IndirectData1,
            0x02 => Self::ProgramCounterLow,
            0x03 => Self::Status,
            0x04 => Self::IndirectAddress0,
            0x05 => Self::Timer0Count,
            0x06 => Self::Timer0Control,
            0x07 => Self::Timer0Reload,
            0x08 => Self::CodecPeriod,
            0x09 => Self::IndirectAddress1,
            0x0a => Self::PinsDirection,
            0x0b => Self::Pins,
            0x0c => Self::CodecControl,
            0x1c => Self::SystemControl,
            0x1d => Self::CommandOut,
            0x1e => Self::CommandIn,
            0x1f => Self::Exchange,
            0x20..=0x3f => return Self::data((value - 0x20) as usize).ok_or(SfrError(value)),
            _ => return Err(SfrError(value)),
        };
        Ok(sfr)
    }
}

impl Sfr {
    pub const fn address(self) -> u8 {
        self as u8
    }

    pub const fn data(index: usize) -> Option<Self> {
        Some(match index {
            0 => Self::Data0,
            1 => Self::Data1,
            2 => Self::Data2,
            3 => Self::Data3,
            4 => Self::Data4,
            5 => Self::Data5,
            6 => Self::Data6,
            7 => Self::Data7,
            8 => Self::Data8,
            9 => Self::Data9,
            10 => Self::Data10,
            11 => Self::Data11,
            12 => Self::Data12,
            13 => Self::Data13,
            14 => Self::Data14,
            15 => Self::Data15,
            16 => Self::Data16,
            17 => Self::Data17,
            18 => Self::Data18,
            19 => Self::Data19,
            20 => Self::Data20,
            21 => Self::Data21,
            22 => Self::Data22,
            23 => Self::Data23,
            24 => Self::Data24,
            25 => Self::Data25,
            26 => Self::Data26,
            27 => Self::Data27,
            28 => Self::Data28,
            29 => Self::Data29,
            30 => Self::Data30,
            31 => Self::Data31,
            _ => return None,
        })
    }

    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::IndirectData0 => "[ptr0]",
            Self::IndirectData1 => "[ptr1]",
            Self::ProgramCounterLow => "pcl",
            Self::Status => "status",
            Self::IndirectAddress0 => "ptr0",
            Self::Timer0Count => "timer0.count",
            Self::Timer0Control => "timer0.ctrl",
            Self::Timer0Reload => "timer0.reload",
            Self::CodecPeriod => "codec.period",
            Self::IndirectAddress1 => "ptr1",
            Self::PinsDirection => "pins.dir",
            Self::Pins => "pins",
            Self::CodecControl => "codec.ctrl",
            Self::SystemControl => "sys.ctrl",
            Self::CommandOut => "cmd.out",
            Self::CommandIn => "cmd.in",
            Self::Exchange => "xchg",
            _ => "data",
        }
    }

    pub const fn legacy_name(self) -> &'static str {
        match self {
            Self::IndirectData0 => "SFR_INDIR_PORT",
            Self::IndirectData1 => "SFR_INDIR_PORT2",
            Self::ProgramCounterLow => "SFR_PRG_COUNT",
            Self::Status => "SFR_STATUS_REG",
            Self::IndirectAddress0 => "SFR_INDIR_ADDR",
            Self::Timer0Count => "SFR_TMR0_COUNT",
            Self::Timer0Control => "SFR_TIMER_CTRL",
            Self::Timer0Reload => "SFR_TMR0_INIT",
            Self::CodecPeriod => "SFR_BIT_CYCLE",
            Self::IndirectAddress1 => "SFR_INDIR_ADDR2",
            Self::PinsDirection => "SFR_PORT_DIR",
            Self::Pins => "SFR_PORT_IO",
            Self::CodecControl => "SFR_BIT_CONFIG",
            Self::SystemControl => "SFR_SYS_CFG",
            Self::CommandOut => "SFR_CTRL_RD",
            Self::CommandIn => "SFR_CTRL_WR",
            Self::Exchange => "SFR_DATA_EXCH",
            _ => "SFR_DATA_REG",
        }
    }

    pub const fn data_index(self) -> Option<u8> {
        let address = self.address();
        if address >= 0x20 {
            Some(address - 0x20)
        } else {
            None
        }
    }
}

/// Canonical dotted name for a documented PIOC SFR bit.
pub const fn canonical_bit_name(register: Register, bit: u8) -> Option<&'static str> {
    Some(match (register.address(), bit) {
        (0x03, 5) => "status.stack_used",
        (0x03, 4) => "status.timeout_reset",
        (0x03, 3) => "status.gp_y",
        (0x03, 2) => "status.zero",
        (0x03, 1) => "status.gp_x",
        (0x03, 0) => "status.carry",
        (0x06, 7) => "timer0.ctrl.level1_enable",
        (0x06, 6) => "timer0.ctrl.level0_enable",
        (0x06, 5) => "timer0.ctrl.enable",
        (0x06, 4) => "timer0.ctrl.output_enable",
        (0x06, 3) => "timer0.ctrl.pwm_mode",
        (0x06, 2) => "timer0.ctrl.divider2",
        (0x06, 1) => "timer0.ctrl.divider1",
        (0x06, 0) => "timer0.ctrl.divider0",
        (0x08, 7) => "codec.period.tx_bit",
        (0x08, 6) => "codec.period.period6",
        (0x08, 5) => "codec.period.period5",
        (0x08, 4) => "codec.period.period4",
        (0x08, 3) => "codec.period.period3",
        (0x08, 2) => "codec.period.period2",
        (0x08, 1) => "codec.period.period1",
        (0x08, 0) => "codec.period.period0",
        (0x0a, 7) => "pins.dir.mode3",
        (0x0a, 6) => "pins.dir.mode2",
        (0x0a, 5) => "pins.dir.mode1",
        (0x0a, 4) => "pins.dir.mode0",
        (0x0a, 3) => "pins.dir.pull1",
        (0x0a, 2) => "pins.dir.pull0",
        (0x0a, 1) => "pins.dir1",
        (0x0a, 0) => "pins.dir0",
        (0x0b, 7) => "pins.input_xor",
        (0x0b, 6) => "codec.rx_bit",
        (0x0b, 5) => "pins.in1",
        (0x0b, 4) => "pins.in0",
        (0x0b, 3) => "pins.changed1",
        (0x0b, 2) => "pins.changed0",
        (0x0b, 1) => "pins.out1",
        (0x0b, 0) => "pins.out0",
        (0x0c, 7) => "codec.ctrl.tx_enable",
        (0x0c, 6) => "codec.ctrl.manchester",
        (0x0c, 5) => "codec.ctrl.edge_sample",
        (0x0c, 4) => "codec.done",
        (0x0c, 3) => "codec.ctrl.count6",
        (0x0c, 2) => "codec.ctrl.count5",
        (0x0c, 1) => "codec.ctrl.count4",
        (0x0c, 0) => "codec.ctrl.count3",
        (0x1c, 7) => "sys.irq_request",
        (0x1c, 6) => "sys.ctrl.core_to_host_pending",
        (0x1c, 5) => "sys.ctrl.host_to_core_pending",
        (0x1c, 4) => "sys.ctrl.host_config",
        (0x1c, 3) => "sys.ctrl.host_claim_io1",
        (0x1c, 2) => "sys.ctrl.host_claim_io0",
        (0x1c, 1) => "sys.ctrl.host_reset",
        (0x1c, 0) => "sys.ctrl.host_clock_run",
        _ => return None,
    })
}

/// Resolve a canonical dotted bitfield name.
pub fn canonical_bit(name: &str) -> Option<(Register, u8)> {
    let name = match name {
        "pins.dir.output1" => "pins.dir1",
        "pins.dir.output0" => "pins.dir0",
        "pins.codec_rx" => "codec.rx_bit",
        "pins.input1" => "pins.in1",
        "pins.input0" => "pins.in0",
        "pins.output1" => "pins.out1",
        "pins.output0" => "pins.out0",
        "codec.ctrl.tail" => "codec.done",
        "sys.ctrl.interrupt_request" => "sys.irq_request",
        _ => name,
    };
    for address in [0x03, 0x06, 0x08, 0x0a, 0x0b, 0x0c, 0x1c] {
        for bit in 0..8 {
            let register = Register::low(address);
            if canonical_bit_name(register, bit).is_some_and(|candidate| candidate == name) {
                return Some((register, bit));
            }
        }
    }
    None
}
