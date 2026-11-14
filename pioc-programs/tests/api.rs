//! Register-level semantics of the protocol drivers, against a recording port.

mod common;

use std::convert::Infallible;
use std::future::Future;

use pioc_programs::ds18x20::{Ds18x20, PowerMode};
use pioc_programs::ws2812::{
    bit_cycles_for_hz, ColorOrder, Config, OutputPin, Rgb8, Storage, Ws2812,
};
use pioc_programs::{CommandPort, Error, Firmware, FirmwareError, PinMask};

const IMAGE: &[u16] = &[0x0000, 0x2855];
const FIRMWARE_48M: Firmware = Firmware {
    words: IMAGE,
    sfr_bit_cycles: 56,
};

#[derive(Debug, PartialEq, Eq)]
enum Operation {
    Reset(PinMask),
    Run(PinMask),
    HostConfig(bool),
    WriteData(u8, Vec<u8>),
    WriteRam(u16, Vec<u8>),
    Command(u8),
    Wait,
}

struct MockPort {
    operations: Vec<Operation>,
    data: [u8; 32],
    ram_capacity: usize,
    status: u8,
}

impl MockPort {
    fn new(ram_capacity: usize) -> Self {
        Self {
            operations: Vec::new(),
            data: [0; 32],
            ram_capacity,
            status: 0,
        }
    }

    fn with_status(status: u8) -> Self {
        Self {
            status,
            ..Self::new(3072)
        }
    }
}

impl CommandPort for MockPort {
    type Error = Infallible;

    fn reset_halted(&mut self, pins: PinMask) -> Result<(), Self::Error> {
        self.operations.push(Operation::Reset(pins));
        Ok(())
    }

    fn run(&mut self, pins: PinMask) -> Result<(), Self::Error> {
        self.operations.push(Operation::Run(pins));
        Ok(())
    }

    fn set_host_config(&mut self, enabled: bool) -> Result<(), Self::Error> {
        self.operations.push(Operation::HostConfig(enabled));
        Ok(())
    }

    fn write_data(&mut self, offset: u8, data: &[u8]) -> Result<(), Self::Error> {
        let start = usize::from(offset);
        self.data[start..start + data.len()].copy_from_slice(data);
        self.operations
            .push(Operation::WriteData(offset, data.to_vec()));
        Ok(())
    }

    fn read_data(&mut self, offset: u8, data: &mut [u8]) -> Result<(), Self::Error> {
        let start = usize::from(offset);
        data.copy_from_slice(&self.data[start..start + data.len()]);
        Ok(())
    }

    fn write_program_ram(&mut self, offset: u16, data: &[u8]) -> Result<(), Self::Error> {
        self.operations
            .push(Operation::WriteRam(offset, data.to_vec()));
        Ok(())
    }

    fn program_ram_capacity(&self) -> usize {
        self.ram_capacity
    }

    fn start_command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.operations.push(Operation::Command(command));
        Ok(())
    }

    fn wait_for_interrupt(&mut self) -> impl Future<Output = Result<u8, Self::Error>> + '_ {
        self.operations.push(Operation::Wait);
        std::future::ready(Ok(self.status))
    }
}

fn twelve_leds() -> [Rgb8; 12] {
    std::array::from_fn(|index| Rgb8::new(index as u8 * 3, 255 - index as u8 * 3, index as u8))
}

#[test]
fn one_rgb_led_uses_the_three_byte_sfr_command_on_the_configured_pin_only() {
    let mut driver = Ws2812::new(MockPort::new(3072), FIRMWARE_48M, Config::default());
    block_on(driver.write_rgb(&[Rgb8::new(255, 0, 0)], ColorOrder::Grb)).unwrap();
    let port = driver.release();

    assert_eq!(
        port.operations,
        [
            Operation::Reset(PinMask::IO0),
            Operation::Run(PinMask::IO0),
            Operation::WriteData(0, vec![0x00, 0xff, 0x00]),
            Operation::Command(3),
            Operation::Wait,
        ]
    );
}

#[test]
fn auto_skips_the_sfr_path_when_the_image_timing_differs() {
    let config = Config {
        bit_cycles: 117,
        ..Config::default()
    };
    let mut driver = Ws2812::new(MockPort::new(3072), FIRMWARE_48M, config);
    block_on(driver.write_rgb(&[Rgb8::new(255, 0, 0)], ColorOrder::Grb)).unwrap();
    let port = driver.release();

    assert!(port
        .operations
        .contains(&Operation::WriteRam(0x400, vec![0x00, 0xff, 0x00])));
    assert!(port.operations.contains(&Operation::Command(0x80 | 117)));
    assert!(!port.operations.contains(&Operation::Command(3)));
}

#[test]
fn explicit_sfr_rejects_an_image_with_a_different_bit_period() {
    let config = Config {
        storage: Storage::Sfr,
        bit_cycles: 117,
        ..Config::default()
    };
    let mut driver = Ws2812::new(MockPort::new(3072), FIRMWARE_48M, config);
    let error = block_on(driver.write_rgb(&[Rgb8::new(255, 0, 0)], ColorOrder::Grb)).unwrap_err();

    assert_eq!(
        error,
        Error::SfrTimingMismatch {
            image: 56,
            configured: 117,
        }
    );
}

#[test]
fn program_ram_use_is_sticky_for_later_short_frames() {
    let mut driver = Ws2812::new(MockPort::new(3072), FIRMWARE_48M, Config::default());
    block_on(driver.write_rgb(&twelve_leds(), ColorOrder::Grb)).unwrap();

    // The image tail is gone, so `Auto` must stay on the RAM path.
    block_on(driver.write_rgb(&[Rgb8::new(1, 2, 3)], ColorOrder::Grb)).unwrap();
    let port = driver.release();

    assert!(port
        .operations
        .contains(&Operation::WriteRam(0x400, vec![0x02, 0x01, 0x03])));
    assert!(!port.operations.contains(&Operation::Command(3)));
}

#[test]
fn switching_back_to_sfr_after_program_ram_reports_the_sticky_rule() {
    let mut driver = Ws2812::new(MockPort::new(3072), FIRMWARE_48M, Config::default());
    block_on(driver.write_rgb(&twelve_leds(), ColorOrder::Grb)).unwrap();
    driver.set_config(Config {
        storage: Storage::Sfr,
        ..Config::default()
    });

    let error = block_on(driver.write_rgb(&[Rgb8::new(1, 2, 3)], ColorOrder::Grb)).unwrap_err();
    assert_eq!(error, Error::SfrUnavailable);
}

#[test]
fn io1_three_byte_collision_automatically_uses_program_ram() {
    let config = Config {
        output: OutputPin::Io1,
        storage: Storage::Auto,
        bit_cycles: 56,
    };
    let mut driver = Ws2812::new(MockPort::new(3072), FIRMWARE_48M, config);
    block_on(driver.write_rgb(&[Rgb8::new(255, 0, 0)], ColorOrder::Grb)).unwrap();
    let port = driver.release();

    assert!(port
        .operations
        .contains(&Operation::WriteRam(0x400, vec![0, 255, 0])));
    assert!(port
        .operations
        .contains(&Operation::WriteData(0, vec![3, 0])));
    assert!(port.operations.contains(&Operation::WriteData(2, vec![1])));
    assert!(port.operations.contains(&Operation::Command(0x80 | 56)));
    // IO1 is claimed on its own.
    assert!(port.operations.contains(&Operation::Reset(PinMask::IO1)));
    assert!(port.operations.contains(&Operation::Run(PinMask::IO1)));
}

#[test]
fn explicit_sfr_mode_reports_the_io1_ds18x20_command_collision() {
    let config = Config {
        output: OutputPin::Io1,
        storage: Storage::Sfr,
        bit_cycles: 56,
    };
    let mut driver = Ws2812::new(MockPort::new(3072), FIRMWARE_48M, config);
    let error = block_on(driver.write_raw(&[0, 255, 0])).unwrap_err();
    assert_eq!(
        error,
        Error::SfrCommandCollision {
            length: 3,
            command: 0x43,
        }
    );
}

#[test]
fn firmware_status_codes_map_to_typed_errors() {
    for (status, expected) in [
        (1, FirmwareError::UnknownCommand),
        (2, FirmwareError::InvalidParameter),
        (4, FirmwareError::OutputHigh),
        (6, FirmwareError::InputHigh),
        (9, FirmwareError::Other(9)),
    ] {
        let mut driver = Ws2812::new(
            MockPort::with_status(status),
            FIRMWARE_48M,
            Config::default(),
        );
        let error = block_on(driver.write_rgb(&[Rgb8::new(1, 2, 3)], ColorOrder::Grb)).unwrap_err();
        assert_eq!(error, Error::Firmware(expected));
    }
}

#[test]
fn payload_limits_are_reported_before_any_transfer_starts() {
    let mut driver = Ws2812::new(MockPort::new(3072), FIRMWARE_48M, Config::default());
    assert_eq!(
        block_on(driver.write_raw(&[])).unwrap_err(),
        Error::EmptyPayload
    );

    let sfr_only = Config {
        storage: Storage::Sfr,
        ..Config::default()
    };
    driver.set_config(sfr_only);
    assert_eq!(
        block_on(driver.write_raw(&[0; 33])).unwrap_err(),
        Error::PayloadTooLong {
            actual: 33,
            maximum: 32,
        }
    );

    // A backend without program RAM cannot fall back from a long frame.
    let mut ramless = Ws2812::new(MockPort::new(0), FIRMWARE_48M, Config::default());
    assert_eq!(
        block_on(ramless.write_rgb(&twelve_leds(), ColorOrder::Grb)).unwrap_err(),
        Error::ProgramRamUnavailable
    );
}

#[test]
fn bit_cycles_follow_the_one_point_one_seven_microsecond_bit() {
    assert_eq!(bit_cycles_for_hz(24_000_000), Some(28));
    assert_eq!(bit_cycles_for_hz(48_000_000), Some(56));
    assert_eq!(bit_cycles_for_hz(100_000_000), Some(117));
    assert_eq!(bit_cycles_for_hz(4_000_000), None);
    assert_eq!(bit_cycles_for_hz(200_000_000), None);
}

#[test]
fn ds18x20_maps_commands_power_and_signed_temperature() {
    let mut port = MockPort::new(0);
    port.data[0..2].copy_from_slice(&(-162_i16).to_le_bytes());
    let mut sensor = Ds18x20::new(port);

    block_on(sensor.start_conversion(PowerMode::Parasite)).unwrap();
    let temperature = block_on(sensor.read_temperature()).unwrap();
    assert_eq!(temperature.raw(), -162);
    assert_eq!(temperature.milli_celsius(), -10_125);

    let port = sensor.release();
    assert_eq!(port.operations[0], Operation::HostConfig(true));
    assert!(port.operations.contains(&Operation::Command(0x41)));
    assert!(port.operations.contains(&Operation::Command(0x42)));
}

#[test]
fn vendor_rgb1w_binary_still_drives_one_led_when_the_evt_tree_is_present() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ch32x035-main/EVT/EXAM/PIOC/1_Wire/Asm/RGB1W.BIN");
    if !path.exists() {
        return;
    }
    let program = pioc_parser::Program::from_le_bytes(&std::fs::read(path).unwrap()).unwrap();
    let firmware = Firmware {
        words: Box::leak(program.words.clone().into_boxed_slice()),
        sfr_bit_cycles: 56,
    };
    let port = common::VmPort::new(firmware.words);
    let mut driver = Ws2812::new(port, firmware, Config::default());

    block_on(driver.write_rgb(&[Rgb8::new(255, 0, 0)], ColorOrder::Grb)).unwrap();
    let port = driver.release();

    let pulses = port.high_pulses();
    let threshold =
        (pulses.iter().min().copied().unwrap_or(0) + pulses.iter().max().copied().unwrap_or(0)) / 2;
    assert_eq!(
        port.bits(threshold),
        common::expected_bits(&[0x00, 0xff, 0x00])
    );
}

fn block_on<F: Future>(future: F) -> F::Output {
    common::block_on(future)
}
