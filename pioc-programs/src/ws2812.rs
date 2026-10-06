//! WS2812/RGB byte-stream driver for the official RGB1W firmware ABI.

use crate::{backend, finish, CommandPort, Error, PinMask};

const SFR_CAPACITY: usize = 32;
const FIRMWARE_RAM_CAPACITY: usize = 3072;
const PROGRAM_RAM_OFFSET: u16 = 0x0400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputPin {
    Io0,
    Io1,
}

impl OutputPin {
    const fn index(self) -> u8 {
        match self {
            Self::Io0 => 0,
            Self::Io1 => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    Auto,
    Sfr,
    ProgramRam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub output: OutputPin,
    pub storage: Storage,
    /// Long/RAM-mode bit period in PIOC clocks. Official values are 56 at
    /// 48 MHz and 28 at 24 MHz.
    pub bit_cycles: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output: OutputPin::Io0,
            storage: Storage::Auto,
            bit_cycles: 56,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorOrder {
    Rgb,
    Rbg,
    #[default]
    Grb,
    Gbr,
    Brg,
    Bgr,
}

impl ColorOrder {
    pub const fn pack(self, color: Rgb8) -> [u8; 3] {
        match self {
            Self::Rgb => [color.r, color.g, color.b],
            Self::Rbg => [color.r, color.b, color.g],
            Self::Grb => [color.g, color.r, color.b],
            Self::Gbr => [color.g, color.b, color.r],
            Self::Brg => [color.b, color.r, color.g],
            Self::Bgr => [color.b, color.g, color.r],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectedStorage {
    Sfr { command: u8 },
    ProgramRam,
}

pub struct Ws2812<B> {
    port: B,
    config: Config,
}

impl<B: CommandPort> Ws2812<B> {
    /// The caller must have loaded the official RGB1W-compatible firmware.
    pub const fn new(port: B, config: Config) -> Self {
        Self { port, config }
    }

    pub fn release(self) -> B {
        self.port
    }

    pub async fn write_raw(&mut self, bytes: &[u8]) -> Result<(), Error<B::Error>> {
        let selected = self.select_storage(bytes.len())?;
        match selected {
            SelectedStorage::Sfr { command } => {
                self.prepare_sfr()?;
                self.port.write_data(0, bytes).map_err(backend)?;
                self.port.start_command(command).map_err(backend)?;
            }
            SelectedStorage::ProgramRam => {
                self.prepare_ram(bytes.len())?;
                self.port
                    .write_program_ram(PROGRAM_RAM_OFFSET, bytes)
                    .map_err(backend)?;
                self.start_ram(bytes.len())?;
            }
        }
        finish(&mut self.port).await
    }

    pub async fn write_rgb(
        &mut self,
        colors: &[Rgb8],
        order: ColorOrder,
    ) -> Result<(), Error<B::Error>> {
        let byte_len = colors.len().checked_mul(3).ok_or(Error::PayloadTooLong {
            actual: usize::MAX,
            maximum: FIRMWARE_RAM_CAPACITY,
        })?;
        let selected = self.select_storage(byte_len)?;
        match selected {
            SelectedStorage::Sfr { command } => {
                self.prepare_sfr()?;
                for (index, color) in colors.iter().copied().enumerate() {
                    self.port
                        .write_data((index * 3) as u8, &order.pack(color))
                        .map_err(backend)?;
                }
                self.port.start_command(command).map_err(backend)?;
            }
            SelectedStorage::ProgramRam => {
                self.prepare_ram(byte_len)?;
                for (index, color) in colors.iter().copied().enumerate() {
                    self.port
                        .write_program_ram(
                            PROGRAM_RAM_OFFSET + (index * 3) as u16,
                            &order.pack(color),
                        )
                        .map_err(backend)?;
                }
                self.start_ram(byte_len)?;
            }
        }
        finish(&mut self.port).await
    }

    fn select_storage(&self, length: usize) -> Result<SelectedStorage, Error<B::Error>> {
        if length == 0 {
            return Err(Error::EmptyPayload);
        }
        let short_command = length as u8
            | match self.config.output {
                OutputPin::Io0 => 0,
                OutputPin::Io1 => 0x40,
            };
        let short_fits = length <= SFR_CAPACITY;
        let short_collides = self.config.output == OutputPin::Io1 && length <= 3;

        match self.config.storage {
            Storage::Sfr if !short_fits => Err(Error::PayloadTooLong {
                actual: length,
                maximum: SFR_CAPACITY,
            }),
            Storage::Sfr if short_collides => Err(Error::SfrCommandCollision {
                length,
                command: short_command,
            }),
            Storage::Sfr => Ok(SelectedStorage::Sfr {
                command: short_command,
            }),
            Storage::Auto if short_fits && !short_collides => Ok(SelectedStorage::Sfr {
                command: short_command,
            }),
            Storage::Auto | Storage::ProgramRam => {
                let maximum = self.port.program_ram_capacity().min(FIRMWARE_RAM_CAPACITY);
                if maximum == 0 {
                    Err(Error::ProgramRamUnavailable)
                } else if length > maximum {
                    Err(Error::PayloadTooLong {
                        actual: length,
                        maximum,
                    })
                } else if !(8..=124).contains(&self.config.bit_cycles) {
                    Err(Error::InvalidBitCycles(self.config.bit_cycles))
                } else {
                    Ok(SelectedStorage::ProgramRam)
                }
            }
        }
    }

    fn prepare_sfr(&mut self) -> Result<(), Error<B::Error>> {
        self.port.reset_halted(PinMask::BOTH).map_err(backend)?;
        self.port.run(PinMask::BOTH).map_err(backend)
    }

    fn prepare_ram(&mut self, _length: usize) -> Result<(), Error<B::Error>> {
        self.port.reset_halted(PinMask::BOTH).map_err(backend)
    }

    fn start_ram(&mut self, length: usize) -> Result<(), Error<B::Error>> {
        self.port.run(PinMask::BOTH).map_err(backend)?;
        self.port
            .write_data(0, &(length as u16).to_le_bytes())
            .map_err(backend)?;
        self.port
            .write_data(2, &[self.config.output.index()])
            .map_err(backend)?;
        self.port
            .start_command(0x80 | self.config.bit_cycles)
            .map_err(backend)
    }
}
