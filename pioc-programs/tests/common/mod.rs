//! VM-backed `CommandPort` for the `pioc-programs` integration tests.

#![allow(dead_code)]

use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use pioc_programs::{CommandPort, PinMask};
use pioc_vm::{Machine, RunState, TraceKind, VmError};

/// Payload bytes the RGB1W ABI exposes through `write_program_ram`.
pub const PROGRAM_RAM_CAPACITY: usize = 3072;

/// Upper bound on VM instructions per interrupt wait.
const STEP_BUDGET: usize = 8_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Reset(PinMask),
    Run(PinMask),
    HostConfig(bool),
    WriteData(u8, Vec<u8>),
    WriteRam(u16, Vec<u8>),
    Command(u8),
    Wait,
}

#[derive(Debug, PartialEq, Eq)]
pub enum VmPortError {
    Vm(VmError),
    Timeout,
}

impl fmt::Display for VmPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vm(error) => write!(formatter, "{error}"),
            Self::Timeout => formatter.write_str("PIOC firmware made no progress"),
        }
    }
}

impl std::error::Error for VmPortError {}

impl From<VmError> for VmPortError {
    fn from(error: VmError) -> Self {
        Self::Vm(error)
    }
}

pub struct VmPort {
    pub machine: Machine,
    /// Host copy of `sys.ctrl`, mirroring the vendor code's cache.
    control: u8,
    pub operations: Vec<Operation>,
}

impl VmPort {
    pub fn new(words: &[u16]) -> Self {
        Self {
            machine: Machine::new(words).expect("firmware image must fit program RAM"),
            control: 0,
            operations: Vec::new(),
        }
    }

    /// High-pulse widths in PIOC cycles, in the order the pin rose.
    pub fn high_pulses(&self) -> Vec<u64> {
        let mut rise = None;
        let mut pulses = Vec::new();
        for event in self.machine.trace() {
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
                        pulses.push(event.cycle - start);
                    }
                }
                _ => {}
            }
        }
        pulses
    }

    /// Decode the IO0 waveform into one bit per WS2812 bit slot.
    pub fn bits(&self, long_threshold: u64) -> Vec<u8> {
        self.high_pulses()
            .into_iter()
            .map(|width| u8::from(width > long_threshold))
            .collect()
    }
}

impl CommandPort for VmPort {
    type Error = VmPortError;

    fn reset_halted(&mut self, pins: PinMask) -> Result<(), Self::Error> {
        self.control = (pins.bits() << 2) | 0x02;
        self.machine.host_write(0x1c, self.control);
        self.operations.push(Operation::Reset(pins));
        Ok(())
    }

    fn run(&mut self, pins: PinMask) -> Result<(), Self::Error> {
        self.control = (pins.bits() << 2) | (self.control & 0x10) | 0x01;
        self.machine.host_write(0x1c, self.control);
        self.operations.push(Operation::Run(pins));
        Ok(())
    }

    fn set_host_config(&mut self, enabled: bool) -> Result<(), Self::Error> {
        if enabled {
            self.control |= 0x10;
        } else {
            self.control &= !0x10;
        }
        self.machine.host_write(0x1c, self.control);
        self.operations.push(Operation::HostConfig(enabled));
        Ok(())
    }

    fn write_data(&mut self, offset: u8, data: &[u8]) -> Result<(), Self::Error> {
        for (index, value) in data.iter().copied().enumerate() {
            self.machine.host_write(0x20 + offset + index as u8, value);
        }
        self.operations
            .push(Operation::WriteData(offset, data.to_vec()));
        Ok(())
    }

    fn read_data(&mut self, offset: u8, data: &mut [u8]) -> Result<(), Self::Error> {
        for (index, value) in data.iter_mut().enumerate() {
            *value = self.machine.host_read(0x20 + offset + index as u8);
        }
        Ok(())
    }

    fn write_program_ram(&mut self, offset: u16, data: &[u8]) -> Result<(), Self::Error> {
        self.machine.write_program_ram(offset, data)?;
        self.operations
            .push(Operation::WriteRam(offset, data.to_vec()));
        Ok(())
    }

    fn program_ram_capacity(&self) -> usize {
        PROGRAM_RAM_CAPACITY
    }

    fn start_command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.machine.host_write(0x1e, command);
        self.operations.push(Operation::Command(command));
        Ok(())
    }

    async fn wait_for_interrupt(&mut self) -> Result<u8, Self::Error> {
        self.operations.push(Operation::Wait);
        for _ in 0..STEP_BUDGET {
            if self.machine.register(0x1c).unwrap_or(0) & 0x80 != 0 {
                // Reading `cmd.out` acknowledges the interrupt.
                return Ok(self.machine.host_read(0x1d));
            }
            match self.machine.state {
                RunState::Running | RunState::Sleeping | RunState::Waiting { .. } => {}
                RunState::ClockGated | RunState::ResetHeld => {
                    return Err(VmPortError::Timeout);
                }
            }
            self.machine.step_or_idle()?;
        }
        Err(VmPortError::Timeout)
    }
}

pub fn block_on<F: Future>(future: F) -> F::Output {
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

/// Expected WS2812 bit stream for a byte payload, MSB first.
pub fn expected_bits(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .flat_map(|byte| (0..8).rev().map(move |bit| (byte >> bit) & 1))
        .collect()
}
