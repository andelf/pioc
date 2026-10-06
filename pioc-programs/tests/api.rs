use std::convert::Infallible;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use pioc_parser::Program;
use pioc_programs::ds18x20::{Ds18x20, PowerMode};
use pioc_programs::ws2812::{ColorOrder, Config, OutputPin, Rgb8, Storage, Ws2812};
use pioc_programs::{CommandPort, Error, PinMask};
use pioc_vm::{Machine, TraceKind, VmError};

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

#[test]
fn one_rgb_led_uses_the_three_byte_sfr_command() {
    let mut driver = Ws2812::new(MockPort::new(3072), Config::default());
    block_on(driver.write_rgb(&[Rgb8::new(255, 0, 0)], ColorOrder::Grb)).unwrap();
    let port = driver.release();

    assert_eq!(
        port.operations,
        [
            Operation::Reset(PinMask::BOTH),
            Operation::Run(PinMask::BOTH),
            Operation::WriteData(0, vec![0x00, 0xff, 0x00]),
            Operation::Command(3),
            Operation::Wait,
        ]
    );
}

#[test]
fn io1_three_byte_collision_automatically_uses_program_ram() {
    let config = Config {
        output: OutputPin::Io1,
        storage: Storage::Auto,
        bit_cycles: 56,
    };
    let mut driver = Ws2812::new(MockPort::new(3072), config);
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
}

#[test]
fn explicit_sfr_mode_reports_the_io1_ds18x20_command_collision() {
    let config = Config {
        output: OutputPin::Io1,
        storage: Storage::Sfr,
        bit_cycles: 56,
    };
    let mut driver = Ws2812::new(MockPort::new(3072), config);
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

#[derive(Debug)]
enum VmPortError {
    Vm(VmError),
    ProgramRamUnsupported,
    Timeout,
}

impl std::fmt::Display for VmPortError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vm(error) => write!(formatter, "{error}"),
            Self::ProgramRamUnsupported => formatter.write_str("program RAM is unavailable"),
            Self::Timeout => formatter.write_str("PIOC completion timeout"),
        }
    }
}

impl std::error::Error for VmPortError {}

struct VmPort {
    machine: Machine,
    control: u8,
}

impl CommandPort for VmPort {
    type Error = VmPortError;

    fn reset_halted(&mut self, pins: PinMask) -> Result<(), Self::Error> {
        self.control = (pins.bits() << 2) | 0x02;
        self.machine.host_write(0x1c, self.control);
        Ok(())
    }

    fn run(&mut self, pins: PinMask) -> Result<(), Self::Error> {
        self.control = (pins.bits() << 2) | (self.control & 0x10) | 0x01;
        self.machine.host_write(0x1c, self.control);
        Ok(())
    }

    fn set_host_config(&mut self, enabled: bool) -> Result<(), Self::Error> {
        if enabled {
            self.control |= 0x10;
        } else {
            self.control &= !0x10;
        }
        self.machine.host_write(0x1c, self.control);
        Ok(())
    }

    fn write_data(&mut self, offset: u8, data: &[u8]) -> Result<(), Self::Error> {
        for (index, value) in data.iter().copied().enumerate() {
            self.machine.host_write(0x20 + offset + index as u8, value);
        }
        Ok(())
    }

    fn read_data(&mut self, offset: u8, data: &mut [u8]) -> Result<(), Self::Error> {
        for (index, value) in data.iter_mut().enumerate() {
            *value = self.machine.host_read(0x20 + offset + index as u8);
        }
        Ok(())
    }

    fn write_program_ram(&mut self, _offset: u16, _data: &[u8]) -> Result<(), Self::Error> {
        Err(VmPortError::ProgramRamUnsupported)
    }

    fn program_ram_capacity(&self) -> usize {
        0
    }

    fn start_command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.machine.host_write(0x1e, command);
        Ok(())
    }

    async fn wait_for_interrupt(&mut self) -> Result<u8, Self::Error> {
        for _ in 0..20_000 {
            if self.machine.register(0x1c).unwrap_or(0) & 0x80 != 0 {
                return Ok(self.machine.host_read(0x1d));
            }
            self.machine.step().map_err(VmPortError::Vm)?;
        }
        Err(VmPortError::Timeout)
    }
}

#[test]
fn high_level_ws2812_api_drives_the_official_rgb1w_binary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ch32x035-main/EVT/EXAM/PIOC/1_Wire/Asm/RGB1W.BIN");
    if !root.exists() {
        return;
    }
    let program = Program::from_le_bytes(&std::fs::read(root).unwrap()).unwrap();
    let machine = Machine::new(&program.words).unwrap();
    let port = VmPort {
        machine,
        control: 0x0d,
    };
    let mut driver = Ws2812::new(port, Config::default());

    block_on(driver.write_rgb(&[Rgb8::new(255, 0, 0)], ColorOrder::Grb)).unwrap();
    let port = driver.release();

    let mut rise = None;
    let mut bits = Vec::new();
    for event in port.machine.trace() {
        if event.kind != TraceKind::Pin
            || event.address != Some(0)
            || event.detail.as_deref() != Some("resolved")
        {
            continue;
        }
        match event.value {
            Some(1) => rise = Some(event.cycle),
            Some(0) => {
                if let Some(start) = rise.take() {
                    bits.push(u8::from(event.cycle - start > 20));
                }
            }
            _ => {}
        }
    }
    let expected = [0x00_u8, 0xff, 0x00]
        .into_iter()
        .flat_map(|byte| (0..8).rev().map(move |bit| (byte >> bit) & 1))
        .collect::<Vec<_>>();
    assert_eq!(bits, expected);
}

fn block_on<F: Future>(future: F) -> F::Output {
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
