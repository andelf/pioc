//! DS18x20 convenience API for the official RGB1W firmware ABI.

use crate::{backend, finish, CommandPort, Error};

const START_CONVERSION: u8 = 0x41;
const READ_TEMPERATURE: u8 = 0x42;
const CONVERT_AND_READ: u8 = 0x43;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    External,
    Parasite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Temperature {
    raw: i16,
}

impl Temperature {
    pub const fn from_raw(raw: i16) -> Self {
        Self { raw }
    }

    pub const fn raw(self) -> i16 {
        self.raw
    }

    pub const fn milli_celsius(self) -> i32 {
        self.raw as i32 * 625 / 10
    }
}

pub struct Ds18x20<B> {
    port: B,
}

impl<B: CommandPort> Ds18x20<B> {
    /// The caller must have loaded and started the official RGB1W firmware.
    pub const fn new(port: B) -> Self {
        Self { port }
    }

    pub fn release(self) -> B {
        self.port
    }

    /// Start conversion and return after the firmware has emitted the command.
    /// The sensor may still require up to 750 ms before it can be read.
    pub async fn start_conversion(&mut self, power: PowerMode) -> Result<(), Error<B::Error>> {
        self.port
            .set_host_config(power == PowerMode::Parasite)
            .map_err(backend)?;
        self.command(START_CONVERSION).await
    }

    pub async fn read_temperature(&mut self) -> Result<Temperature, Error<B::Error>> {
        self.command(READ_TEMPERATURE).await?;
        self.read_result()
    }

    /// Use firmware command 43h, which performs conversion, waits 750 ms in
    /// the PIOC, and reads the first two scratchpad bytes.
    pub async fn convert_and_read(
        &mut self,
        power: PowerMode,
    ) -> Result<Temperature, Error<B::Error>> {
        self.port
            .set_host_config(power == PowerMode::Parasite)
            .map_err(backend)?;
        self.command(CONVERT_AND_READ).await?;
        self.read_result()
    }

    async fn command(&mut self, command: u8) -> Result<(), Error<B::Error>> {
        self.port.start_command(command).map_err(backend)?;
        finish(&mut self.port).await
    }

    fn read_result(&mut self) -> Result<Temperature, Error<B::Error>> {
        let mut bytes = [0; 2];
        self.port.read_data(0, &mut bytes).map_err(backend)?;
        Ok(Temperature::from_raw(i16::from_le_bytes(bytes)))
    }
}
