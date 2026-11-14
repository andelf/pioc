//! WS2812/RGB byte-stream driver for the official RGB1W firmware ABI.

use crate::{backend, finish, CommandPort, Error, Firmware, PinMask};

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

    /// Claim only the pin the stream is driven from.
    const fn mask(self) -> PinMask {
        match self {
            Self::Io0 => PinMask::IO0,
            Self::Io1 => PinMask::IO1,
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
    /// RAM-mode bit period in PIOC clocks.
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
    firmware: Firmware,
    config: Config,
    /// A RAM payload has overwritten the image tail.
    program_ram_dirty: bool,
}

impl<B: CommandPort> Ws2812<B> {
    /// The caller must have loaded `firmware` into the PIOC program SRAM.
    pub const fn new(port: B, firmware: Firmware, config: Config) -> Self {
        Self {
            port,
            firmware,
            config,
            program_ram_dirty: false,
        }
    }

    pub fn release(self) -> B {
        self.port
    }

    /// Replace the configuration.
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn config(&self) -> Config {
        self.config
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
        let sfr_timing_matches = self.firmware.sfr_bit_cycles == self.config.bit_cycles;

        match self.config.storage {
            Storage::Sfr if self.program_ram_dirty => Err(Error::SfrUnavailable),
            Storage::Sfr if !short_fits => Err(Error::PayloadTooLong {
                actual: length,
                maximum: SFR_CAPACITY,
            }),
            Storage::Sfr if short_collides => Err(Error::SfrCommandCollision {
                length,
                command: short_command,
            }),
            Storage::Sfr if !sfr_timing_matches => Err(Error::SfrTimingMismatch {
                image: self.firmware.sfr_bit_cycles,
                configured: self.config.bit_cycles,
            }),
            Storage::Sfr => Ok(SelectedStorage::Sfr {
                command: short_command,
            }),
            // Fall back to the RAM path when the SFR image is unusable.
            Storage::Auto
                if short_fits
                    && !short_collides
                    && !self.program_ram_dirty
                    && sfr_timing_matches =>
            {
                Ok(SelectedStorage::Sfr {
                    command: short_command,
                })
            }
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
        let pins = self.config.output.mask();
        self.port.reset_halted(pins).map_err(backend)?;
        self.port.run(pins).map_err(backend)
    }

    fn prepare_ram(&mut self, _length: usize) -> Result<(), Error<B::Error>> {
        self.program_ram_dirty = true;
        self.port
            .reset_halted(self.config.output.mask())
            .map_err(backend)
    }

    fn start_ram(&mut self, length: usize) -> Result<(), Error<B::Error>> {
        let pins = self.config.output.mask();
        self.port.run(pins).map_err(backend)?;
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

/// Bit period in PIOC clocks for the 1.17 µs WS2812 bit at `hz` clocks.
///
/// Returns `None` outside the 8..=124 range accepted by the RAM command.
pub const fn bit_cycles_for_hz(hz: u32) -> Option<u8> {
    const BIT_PERIOD_NANOS: u64 = 1_170;
    let cycles = (BIT_PERIOD_NANOS * hz as u64 + 500_000_000) / 1_000_000_000;
    if cycles >= 8 && cycles <= 124 {
        Some(cycles as u8)
    } else {
        None
    }
}
