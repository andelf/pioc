#![no_std]
//! Embassy-style protocol drivers over a WCH PIOC command transport.

use core::future::Future;

pub mod ds18x20;
pub mod ws2812;

/// PIOC pin ownership requested while a firmware image is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinMask(u8);

impl PinMask {
    pub const IO0: Self = Self(1);
    pub const IO1: Self = Self(2);
    pub const BOTH: Self = Self(3);

    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// Low-level operations required by protocol drivers.
///
/// A hardware implementation maps completion waiting to the PIOC interrupt.
/// A simulator implementation can instead execute until firmware raises the
/// same interrupt-request bit.
pub trait CommandPort {
    type Error;

    /// Assert reset, stop the PIOC clock, and claim the selected pins.
    fn reset_halted(&mut self, pins: PinMask) -> Result<(), Self::Error>;

    /// Release reset and run the PIOC clock with the selected pins claimed.
    fn run(&mut self, pins: PinMask) -> Result<(), Self::Error>;

    /// Set `sys.ctrl.host_config`, used by RGB1W as the parasite-power flag.
    fn set_host_config(&mut self, enabled: bool) -> Result<(), Self::Error>;

    /// Access shared registers `d0..d31`.
    fn write_data(&mut self, offset: u8, data: &[u8]) -> Result<(), Self::Error>;
    fn read_data(&mut self, offset: u8, data: &mut [u8]) -> Result<(), Self::Error>;

    /// Write firmware-owned shared SRAM while PIOC is halted.
    fn write_program_ram(&mut self, offset: u16, data: &[u8]) -> Result<(), Self::Error>;

    /// Bytes available to protocol payloads through `write_program_ram`.
    fn program_ram_capacity(&self) -> usize;

    /// Write `cmd.in`, starting a firmware operation.
    fn start_command(&mut self, command: u8) -> Result<(), Self::Error>;

    /// Wait for a PIOC interrupt, acknowledge it according to the backend's
    /// hardware rules, and return the byte observed in `cmd.out`.
    ///
    /// Command-oriented firmware treats this as completion. Streaming
    /// firmware may call it repeatedly for refill and receive events.
    fn wait_for_interrupt(&mut self) -> impl Future<Output = Result<u8, Self::Error>> + '_;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareError {
    UnknownCommand,
    InvalidParameter,
    OutputHigh,
    InputHigh,
    Other(u8),
}

impl FirmwareError {
    pub const fn from_status(status: u8) -> Option<Self> {
        match status {
            0 => None,
            1 => Some(Self::UnknownCommand),
            2 => Some(Self::InvalidParameter),
            4 => Some(Self::OutputHigh),
            6 => Some(Self::InputHigh),
            other => Some(Self::Other(other)),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error<E> {
    Backend(E),
    EmptyPayload,
    PayloadTooLong { actual: usize, maximum: usize },
    SfrCommandCollision { length: usize, command: u8 },
    ProgramRamUnavailable,
    InvalidBitCycles(u8),
    Firmware(FirmwareError),
}

pub(crate) fn backend<E>(error: E) -> Error<E> {
    Error::Backend(error)
}

pub(crate) async fn finish<B: CommandPort>(port: &mut B) -> Result<(), Error<B::Error>> {
    let status = port.wait_for_interrupt().await.map_err(backend)?;
    match FirmwareError::from_status(status) {
        None => Ok(()),
        Some(error) => Err(Error::Firmware(error)),
    }
}
