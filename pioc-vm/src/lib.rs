//! Cycle-aware PIOC virtual machine.

use pioc_core::{
    BitInput, BitIo, DecodeTarget, Destination, Instruction, Register, WaitEvent, PROGRAM_WORDS,
    RESET_PC,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod simulation;
mod waveform;
pub use simulation::{run_scenario, EventAction, RunLimits, RunReport, ScheduledEvent, StopReason};
pub use waveform::{to_vcd, to_wavejson, WaveError, WaveOptions};

const STATUS: usize = 0x03;
const INDIRECT_DATA0: usize = 0x00;
const INDIRECT_DATA1: usize = 0x01;
const PCL: usize = 0x02;
const INDIRECT_ADDRESS0: usize = 0x04;
const TIMER_COUNT: usize = 0x05;
const TIMER_CONTROL: usize = 0x06;
const TIMER_RELOAD: usize = 0x07;
const INDIRECT_ADDRESS1: usize = 0x09;
const PIN_DIRECTION: usize = 0x0a;
const PINS: usize = 0x0b;
const CODEC_PERIOD: usize = 0x08;
const CODEC_CONTROL: usize = 0x0c;
const SYSTEM_CONTROL: usize = 0x1c;
const COMMAND_OUT: usize = 0x1d;
const COMMAND_IN: usize = 0x1e;
const EXCHANGE: usize = 0x1f;

const STATUS_STACK_USED: u8 = 1 << 5;
const STATUS_TIMEOUT_RESET: u8 = 1 << 4;
const STATUS_ZERO: u8 = 1 << 2;
const STATUS_CARRY: u8 = 1;
const SYS_CORE_TO_HOST_PENDING: u8 = 1 << 6;
const SYS_HOST_TO_CORE_PENDING: u8 = 1 << 5;
const SYS_INTERRUPT_REQUEST: u8 = 1 << 7;
const SYS_HOST_CLAIM_IO1: u8 = 1 << 3;
const SYS_HOST_CLAIM_IO0: u8 = 1 << 2;
const SYS_HOST_RESET: u8 = 1 << 1;
const SYS_HOST_CLOCK_RUN: u8 = 1;
const TIMER_ENABLE: u8 = 1 << 5;
const TIMER_OUTPUT_ENABLE: u8 = 1 << 4;
const TIMER_PWM_MODE: u8 = 1 << 3;
const CODEC_TX_ENABLE: u8 = 1 << 7;
const CODEC_MANCHESTER: u8 = 1 << 6;
const CODEC_TAIL: u8 = 1 << 4;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Running,
    Waiting { event: u8 },
    Sleeping,
    ClockGated,
    ResetHeld,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TraceKind {
    Instruction,
    RegisterRead,
    RegisterWrite,
    HostRead,
    HostWrite,
    Pin,
    State,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceEvent {
    pub cycle: u64,
    pub kind: TraceKind,
    pub pc: Option<u16>,
    pub word: Option<u16>,
    pub address: Option<u16>,
    pub value: Option<u8>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepOutcome {
    pub cycles: u8,
    pub state: RunState,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VmError {
    #[error("program contains {actual} words; PIOC supports at most {maximum}")]
    ProgramTooLarge { actual: usize, maximum: usize },
    #[error("program counter 0x{pc:03x} is outside loaded program")]
    ProgramCounter { pc: u16 },
    #[error("instruction {instruction:?} is target-specific or unsupported")]
    UnsupportedInstruction { instruction: Instruction },
    #[error("stack overflow at depth 6")]
    StackOverflow,
    #[error("stack underflow")]
    StackUnderflow,
    #[error("stack entry does not match {operation}")]
    StackType { operation: &'static str },
    #[error("scenario event at cycle {cycle} appears after an event at cycle {previous}")]
    EventOrder { previous: u64, cycle: u64 },
    #[error("scenario pin {pin} is invalid; PIOC has pins 0 and 1")]
    InvalidPin { pin: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StackEntry {
    Return(u16),
    AccumulatorStatus { accumulator: u8, status: u8 },
    Indirect1 { address: u16 },
}

#[derive(Debug, Clone)]
pub struct Machine {
    pub pc: u16,
    pub accumulator: u8,
    pub carry: bool,
    pub zero: bool,
    pub cycles: u64,
    pub state: RunState,
    program: Vec<u16>,
    registers: [u8; 0x400],
    indirect_address0: u16,
    indirect_address1: u16,
    stack: Vec<StackEntry>,
    external_pins: [Option<bool>; 2],
    resolved_pins: [Option<bool>; 2],
    core_pin_outputs: [Option<bool>; 2],
    timer_divider_phase: u16,
    timer_output: bool,
    codec_phase: u8,
    codec_latched_bit: bool,
    codec_input_start: bool,
    codec_output: bool,
    codec_active: bool,
    reset_requested: bool,
    io0_rising: bool,
    io0_falling: bool,
    trace: Vec<TraceEvent>,
}

impl Machine {
    pub fn new(program: &[u16]) -> Result<Self, VmError> {
        if program.len() > PROGRAM_WORDS {
            return Err(VmError::ProgramTooLarge {
                actual: program.len(),
                maximum: PROGRAM_WORDS,
            });
        }
        let mut machine = Self {
            pc: RESET_PC,
            accumulator: 0,
            carry: false,
            zero: false,
            cycles: 0,
            state: RunState::Running,
            program: program.to_vec(),
            registers: [0; 0x400],
            indirect_address0: 0,
            indirect_address1: 0,
            stack: Vec::with_capacity(6),
            external_pins: [None; 2],
            resolved_pins: [None; 2],
            core_pin_outputs: [None; 2],
            timer_divider_phase: 0,
            timer_output: false,
            codec_phase: 0,
            codec_latched_bit: false,
            codec_input_start: false,
            codec_output: false,
            codec_active: false,
            reset_requested: false,
            io0_rising: false,
            io0_falling: false,
            trace: Vec::new(),
        };
        machine.registers[CODEC_CONTROL] = CODEC_TAIL;
        // Standalone machines start runnable and own both pins. Host scenarios
        // may subsequently exercise reset, ownership, and clock gating.
        machine.registers[SYSTEM_CONTROL] =
            SYS_HOST_CLOCK_RUN | SYS_HOST_CLAIM_IO0 | SYS_HOST_CLAIM_IO1;
        machine.sync_status();
        Ok(machine)
    }

    pub fn step(&mut self) -> Result<StepOutcome, VmError> {
        let state = self.state;
        match state {
            RunState::Sleeping | RunState::ClockGated | RunState::ResetHeld => {
                return Ok(StepOutcome {
                    cycles: 0,
                    state: self.state,
                });
            }
            RunState::Waiting { event } => {
                if !self.wait_ready(WaitEvent::from_bits(event)) {
                    return Ok(StepOutcome {
                        cycles: 0,
                        state: self.state,
                    });
                }
                let timeout_was_active = self.timeout_timer_active();
                self.state = RunState::Running;
                if timeout_was_active {
                    self.registers[TIMER_COUNT] = self.registers[TIMER_RELOAD];
                    self.timer_divider_phase = 0;
                }
            }
            RunState::Running => {}
        }

        let word = self
            .program
            .get(self.pc as usize)
            .copied()
            .ok_or(VmError::ProgramCounter { pc: self.pc })?;
        let decoded = Instruction::decode(word, DecodeTarget::Ch32x035Pioc);
        self.trace.push(TraceEvent {
            cycle: self.cycles,
            kind: TraceKind::Instruction,
            pc: Some(self.pc),
            word: Some(word),
            address: None,
            value: None,
            detail: Some(format!("{:?}", decoded.instruction)),
        });

        let instruction = decoded.instruction;
        let mut next_pc = self.pc.wrapping_add(1) & 0x07ff;
        let mut instruction_cycles = 1;
        self.execute(instruction, &mut next_pc, &mut instruction_cycles)?;
        let (elapsed, reset) = self.advance_peripherals(instruction_cycles);
        if !reset {
            self.pc = next_pc;
        }
        self.cycles += u64::from(elapsed);
        self.sync_status();
        Ok(StepOutcome {
            cycles: elapsed,
            state: self.state,
        })
    }

    pub fn run_steps(&mut self, maximum: usize) -> Result<usize, VmError> {
        let mut executed = 0;
        while executed < maximum && self.state == RunState::Running {
            self.step()?;
            executed += 1;
        }
        Ok(executed)
    }

    pub fn register(&mut self, address: u16) -> Option<u8> {
        Register::new(address).map(|register| self.read_register(register))
    }

    pub fn set_external_pin(&mut self, pin: usize, value: bool) {
        self.set_external_drive(pin, Some(value));
    }

    /// Drive an external pin low/high, or release it with `None`.
    pub fn set_external_drive(&mut self, pin: usize, value: Option<bool>) {
        if pin >= 2 {
            return;
        }
        self.external_pins[pin] = value;
        self.refresh_pin_inputs();
        self.trace.push(TraceEvent {
            cycle: self.cycles,
            kind: TraceKind::Pin,
            pc: None,
            word: None,
            address: Some(pin as u16),
            value: value.map(u8::from),
            detail: Some("external".into()),
        });
    }

    pub fn pin_level(&self, pin: usize) -> Option<bool> {
        self.resolved_pins.get(pin).copied().flatten()
    }

    pub fn host_write(&mut self, address: u8, value: u8) {
        match address as usize {
            COMMAND_IN => {
                self.registers[COMMAND_IN] = value;
                self.registers[SYSTEM_CONTROL] |= SYS_HOST_TO_CORE_PENDING;
            }
            COMMAND_OUT => {
                self.registers[SYSTEM_CONTROL] &=
                    !(SYS_CORE_TO_HOST_PENDING | SYS_INTERRUPT_REQUEST);
            }
            SYSTEM_CONTROL => {
                self.registers[SYSTEM_CONTROL] =
                    (self.registers[SYSTEM_CONTROL] & 0xe0) | (value & 0x1f);
                if value & SYS_HOST_RESET != 0 {
                    self.reset_core(self.cycles, "host_reset");
                    self.state = RunState::ResetHeld;
                } else if value & SYS_HOST_CLOCK_RUN == 0 {
                    self.state = RunState::ClockGated;
                } else if matches!(self.state, RunState::ClockGated | RunState::ResetHeld) {
                    self.state = RunState::Running;
                }
                self.record_core_pin_changes(self.cycles);
            }
            _ => self.registers[address as usize] = value,
        }
        self.trace.push(TraceEvent {
            cycle: self.cycles,
            kind: TraceKind::HostWrite,
            pc: None,
            word: None,
            address: Some(address as u16),
            value: Some(value),
            detail: None,
        });
    }

    pub fn host_read(&mut self, address: u8) -> u8 {
        let value = self.registers[address as usize];
        if address as usize == COMMAND_OUT {
            self.registers[SYSTEM_CONTROL] &= !SYS_CORE_TO_HOST_PENDING;
        }
        self.trace.push(TraceEvent {
            cycle: self.cycles,
            kind: TraceKind::HostRead,
            pc: None,
            word: None,
            address: Some(address as u16),
            value: Some(value),
            detail: None,
        });
        value
    }

    pub fn trace(&self) -> &[TraceEvent] {
        &self.trace
    }

    pub fn take_trace(&mut self) -> Vec<TraceEvent> {
        core::mem::take(&mut self.trace)
    }

    fn execute(
        &mut self,
        instruction: Instruction,
        next_pc: &mut u16,
        instruction_cycles: &mut u8,
    ) -> Result<(), VmError> {
        use Instruction::*;
        match instruction {
            Unknown(_) | WriteCode | Exec { .. } => {
                return Err(VmError::UnsupportedInstruction { instruction });
            }
            Nop | ClearWatchdog => {}
            ClearAccumulator => {
                self.accumulator = 0;
                self.zero = true;
            }
            Sleep { .. } => self.state = RunState::Sleeping,
            Wait(event) => {
                if self.wait_ready(event) {
                    self.consume_wait_event(event);
                } else {
                    self.state = RunState::Waiting { event: event as u8 };
                    *next_pc = self.pc;
                }
            }
            ReadCode { space } => {
                // PIOC exposes one loaded image. Target-specific space selectors
                // alias that image until a device supplies additional banks.
                let _ = space;
                let address =
                    ((u16::from(self.accumulator) & 7) << 8) | (self.indirect_address0 & 0xff);
                let word = self.program.get(address as usize).copied().unwrap_or(0);
                self.accumulator = word as u8;
                self.set_indirect0_low((word >> 8) as u8);
                *instruction_cycles = 3;
            }
            BitToCarry(input) => self.carry = self.read_bit_input(input),
            PushAccumulatorStatus => self.push(StackEntry::AccumulatorStatus {
                accumulator: self.accumulator,
                status: self.registers[STATUS],
            })?,
            PopAccumulatorStatus => match self.pop()? {
                StackEntry::AccumulatorStatus {
                    accumulator,
                    status,
                } => {
                    self.accumulator = accumulator;
                    self.registers[STATUS] = status;
                    self.load_flags_from_status();
                }
                _ => return Err(VmError::StackType { operation: "POPAS" }),
            },
            PushIndirect1 => self.push(StackEntry::Indirect1 {
                address: self.indirect_address1,
            })?,
            PopIndirect1 => match self.pop()? {
                StackEntry::Indirect1 { address } => self.set_indirect1(address),
                _ => return Err(VmError::StackType { operation: "POPA2" }),
            },
            Return => {
                *next_pc = self.pop_return()?;
                *instruction_cycles = 2;
            }
            ReturnZero => {
                self.zero = true;
                *next_pc = self.pop_return()?;
                *instruction_cycles = 2;
            }
            ReturnInterrupt => {
                *next_pc = self.pop_return()?;
                *instruction_cycles = 2;
            }
            ReturnLiteral(value) => {
                self.accumulator = value;
                *next_pc = self.pop_return()?;
                *instruction_cycles = 2;
            }
            ReturnLiteralNotZero(value) => {
                self.accumulator = value;
                self.zero = false;
                *next_pc = self.pop_return()?;
                *instruction_cycles = 2;
            }
            Clear(register) => {
                self.write_register(register, 0, next_pc);
                self.zero = true;
                if register.address() == PCL as u16 {
                    *instruction_cycles = 2;
                }
            }
            StoreAccumulator(register) => {
                self.write_register(register, self.accumulator, next_pc);
                if register.address() == INDIRECT_DATA1 as u16 {
                    self.set_indirect1(self.indirect_address1.wrapping_add(1) & 0x03ff);
                }
                if register.address() == PCL as u16 {
                    *instruction_cycles = 2;
                }
            }
            Load {
                source,
                destination,
            } => {
                let value = self.read_register(source);
                self.write_destination(destination, source, value, next_pc);
                if source.address() == INDIRECT_DATA1 as u16 {
                    self.set_indirect1(self.indirect_address1.wrapping_add(1) & 0x03ff);
                }
                self.zero = value == 0;
                if destination == Destination::Register && source.address() == PCL as u16 {
                    *instruction_cycles = 2;
                }
            }
            Increment {
                source,
                destination,
            } => {
                let value = self.read_register(source).wrapping_add(1);
                self.write_destination(destination, source, value, next_pc);
                self.zero = value == 0;
            }
            Decrement {
                source,
                destination,
            } => {
                let value = self.read_register(source).wrapping_sub(1);
                self.write_destination(destination, source, value, next_pc);
                self.zero = value == 0;
            }
            IncrementSkipZero {
                source,
                destination,
            } => {
                let value = self.read_register(source).wrapping_add(1);
                self.write_destination(destination, source, value, next_pc);
                if value == 0 {
                    *next_pc = next_pc.wrapping_add(1) & 0x07ff;
                    *instruction_cycles = 2;
                }
            }
            DecrementSkipZero {
                source,
                destination,
            } => {
                let value = self.read_register(source).wrapping_sub(1);
                self.write_destination(destination, source, value, next_pc);
                if value == 0 {
                    *next_pc = next_pc.wrapping_add(1) & 0x07ff;
                    *instruction_cycles = 2;
                }
            }
            SwapNibbles {
                source,
                destination,
            } => {
                let value = self.read_register(source).rotate_left(4);
                self.write_destination(destination, source, value, next_pc);
            }
            And {
                source,
                destination,
            } => {
                let value = self.accumulator & self.read_register(source);
                self.write_destination(destination, source, value, next_pc);
                self.zero = value == 0;
            }
            Or {
                source,
                destination,
            } => {
                let value = self.accumulator | self.read_register(source);
                self.write_destination(destination, source, value, next_pc);
                self.zero = value == 0;
            }
            Xor {
                source,
                destination,
            } => {
                let value = self.accumulator ^ self.read_register(source);
                self.write_destination(destination, source, value, next_pc);
                self.zero = value == 0;
            }
            Add {
                source,
                destination,
            } => {
                let (value, carry) = self.accumulator.overflowing_add(self.read_register(source));
                self.write_destination(destination, source, value, next_pc);
                self.zero = value == 0;
                self.carry = carry;
            }
            Subtract {
                source,
                destination,
            } => {
                let (value, borrow) = self.read_register(source).overflowing_sub(self.accumulator);
                self.write_destination(destination, source, value, next_pc);
                self.zero = value == 0;
                self.carry = borrow;
            }
            RotateLeftCarry {
                source,
                destination,
            } => {
                let original = self.read_register(source);
                let value = (original << 1) | u8::from(self.carry);
                self.carry = original & 0x80 != 0;
                self.write_destination(destination, source, value, next_pc);
            }
            RotateRightCarry {
                source,
                destination,
            } => {
                let original = self.read_register(source);
                let value = (original >> 1) | (u8::from(self.carry) << 7);
                self.carry = original & 1 != 0;
                self.write_destination(destination, source, value, next_pc);
            }
            MoveIndirect0(address) => self.set_indirect0(address),
            MoveIndirect1(address) => self.set_indirect1(address),
            MoveLiteralFast1(value) => self.registers[PIN_DIRECTION] = value,
            MoveLiteralFast2(value) => self.write_raw(PINS, value),
            MoveLiteralIndirect1(value) => {
                self.registers[self.indirect_address1 as usize] = value;
                self.set_indirect1(self.indirect_address1.wrapping_add(1) & 0x03ff);
            }
            MoveLiteralIndirect0(value) => {
                self.registers[self.indirect_address0 as usize] = value;
            }
            MoveLiteral(value) => self.accumulator = value,
            AndLiteral(value) => {
                self.accumulator &= value;
                self.zero = self.accumulator == 0;
            }
            OrLiteral(value) => {
                self.accumulator |= value;
                self.zero = self.accumulator == 0;
            }
            XorLiteral(value) => {
                self.accumulator ^= value;
                self.zero = self.accumulator == 0;
            }
            AddLiteral(value) => {
                let (result, carry) = self.accumulator.overflowing_add(value);
                self.accumulator = result;
                self.zero = result == 0;
                self.carry = carry;
            }
            SubtractLiteral(value) => {
                let (result, borrow) = value.overflowing_sub(self.accumulator);
                self.accumulator = result;
                self.zero = result == 0;
                self.carry = borrow;
            }
            CompareAddLiteral(value) => {
                let (result, carry) = value.overflowing_add(self.accumulator);
                self.zero = result == 0;
                self.carry = carry;
            }
            CompareSubtractLiteral(value) => {
                let (result, borrow) = value.overflowing_sub(self.accumulator);
                self.zero = result == 0;
                self.carry = borrow;
            }
            BitClear { register, bit } => {
                let value = self.read_register(register) & !(1 << bit);
                self.write_register(register, value, next_pc);
            }
            BitSet { register, bit } => {
                let value = self.read_register(register) | (1 << bit);
                self.write_register(register, value, next_pc);
            }
            BitTestSkipClear { register, bit } => {
                if self.read_register(register) & (1 << bit) == 0 {
                    *next_pc = next_pc.wrapping_add(1) & 0x07ff;
                    *instruction_cycles = 2;
                }
            }
            BitTestSkipSet { register, bit } => {
                if self.read_register(register) & (1 << bit) != 0 {
                    *next_pc = next_pc.wrapping_add(1) & 0x07ff;
                    *instruction_cycles = 2;
                }
            }
            BitFromIndirect0 { destination, bit } => {
                let value = self.registers[INDIRECT_ADDRESS0] & (1 << bit) != 0;
                self.write_bit_endpoint(destination, value);
            }
            BitFromExchange { destination, bit } => {
                let value = self.registers[EXCHANGE] & (1 << bit) != 0;
                self.write_bit_endpoint(destination, value);
            }
            BitToIndirect0 { source, bit } => {
                let value = self.read_bit_endpoint(source);
                set_bit(&mut self.registers[INDIRECT_ADDRESS0], bit, value);
                self.set_indirect0_low(self.registers[INDIRECT_ADDRESS0]);
            }
            BitToExchange { source, bit } => {
                let value = self.read_bit_endpoint(source);
                set_bit(&mut self.registers[EXCHANGE], bit, value);
            }
            Jump(target) => {
                *next_pc = target & 0x07ff;
                *instruction_cycles = 2;
            }
            Call(target) => {
                self.push(StackEntry::Return(*next_pc))?;
                *next_pc = target & 0x07ff;
                *instruction_cycles = 2;
            }
            JumpNotZero(target) => branch(!self.zero, target, 0x03ff, next_pc, instruction_cycles),
            JumpZero(target) => branch(self.zero, target, 0x03ff, next_pc, instruction_cycles),
            JumpNotCarry(target) => {
                branch(!self.carry, target, 0x03ff, next_pc, instruction_cycles)
            }
            JumpCarry(target) => branch(self.carry, target, 0x03ff, next_pc, instruction_cycles),
            CompareJump { literal, target } => branch(
                self.accumulator == literal,
                u16::from(target),
                0x00ff,
                next_pc,
                instruction_cycles,
            ),
        }
        if writes_program_counter_low(instruction) {
            *instruction_cycles = (*instruction_cycles).max(2);
        }
        Ok(())
    }

    fn read_register(&mut self, register: Register) -> u8 {
        let address = register.address() as usize;
        let value = match address {
            INDIRECT_DATA0 => self.registers[self.indirect_address0 as usize],
            INDIRECT_DATA1 => self.registers[self.indirect_address1 as usize],
            PCL => self.pc as u8,
            STATUS => {
                self.sync_status();
                self.registers[STATUS]
            }
            PINS => {
                self.refresh_pin_inputs();
                self.registers[PINS]
            }
            COMMAND_IN => {
                self.registers[SYSTEM_CONTROL] &= !SYS_HOST_TO_CORE_PENDING;
                self.registers[COMMAND_IN]
            }
            _ => self.registers[address],
        };
        self.trace.push(TraceEvent {
            cycle: self.cycles,
            kind: TraceKind::RegisterRead,
            pc: Some(self.pc),
            word: None,
            address: Some(address as u16),
            value: Some(value),
            detail: None,
        });
        value
    }

    fn write_register(&mut self, register: Register, value: u8, next_pc: &mut u16) {
        let address = register.address() as usize;
        match address {
            INDIRECT_DATA0 => {
                self.registers[self.indirect_address0 as usize] = value;
            }
            INDIRECT_DATA1 => {
                self.registers[self.indirect_address1 as usize] = value;
            }
            PCL => {
                *next_pc = (*next_pc & !0xff) | u16::from(value);
                self.registers[PCL] = value;
            }
            STATUS => {
                self.registers[STATUS] = value;
                self.load_flags_from_status();
            }
            INDIRECT_ADDRESS0 => self.set_indirect0_low(value),
            INDIRECT_ADDRESS1 => self.set_indirect1_low(value),
            CODEC_PERIOD => {
                let period_changed = (self.registers[CODEC_PERIOD] ^ value) & 0x7f != 0;
                self.registers[CODEC_PERIOD] = value;
                self.io0_rising = false;
                self.io0_falling = false;
                if period_changed {
                    self.codec_active = false;
                }
            }
            CODEC_CONTROL => {
                let mode_changed = (self.registers[CODEC_CONTROL] ^ value) & 0xc0 != 0;
                self.registers[CODEC_CONTROL] =
                    (value & 0xe0) | (self.registers[CODEC_CONTROL] & 0x1f);
                if mode_changed {
                    self.codec_active = false;
                }
            }
            COMMAND_OUT => {
                self.registers[COMMAND_OUT] = value;
                self.registers[SYSTEM_CONTROL] |= SYS_CORE_TO_HOST_PENDING;
            }
            SYSTEM_CONTROL => {
                // Host owns bits 0..=4; core may only change request/status bits.
                self.registers[SYSTEM_CONTROL] =
                    (self.registers[SYSTEM_CONTROL] & 0x1f) | (value & 0xe0);
            }
            _ => self.registers[address] = value,
        }
        self.trace.push(TraceEvent {
            cycle: self.cycles,
            kind: TraceKind::RegisterWrite,
            pc: Some(self.pc),
            word: None,
            address: Some(address as u16),
            value: Some(value),
            detail: None,
        });
    }

    fn write_raw(&mut self, address: usize, value: u8) {
        self.registers[address] = value;
    }

    fn set_indirect0(&mut self, address: u16) {
        self.indirect_address0 = address & 0x01ff;
        self.registers[INDIRECT_ADDRESS0] = self.indirect_address0 as u8;
    }

    fn set_indirect0_low(&mut self, value: u8) {
        self.set_indirect0((self.indirect_address0 & 0x0100) | u16::from(value));
    }

    fn set_indirect1(&mut self, address: u16) {
        self.indirect_address1 = address & 0x03ff;
        self.registers[INDIRECT_ADDRESS1] = self.indirect_address1 as u8;
    }

    fn set_indirect1_low(&mut self, value: u8) {
        self.set_indirect1((self.indirect_address1 & 0x0300) | u16::from(value));
    }

    fn write_destination(
        &mut self,
        destination: Destination,
        register: Register,
        value: u8,
        next_pc: &mut u16,
    ) {
        match destination {
            Destination::Accumulator => self.accumulator = value,
            Destination::Register => self.write_register(register, value, next_pc),
        }
    }

    fn read_bit_input(&self, input: BitInput) -> bool {
        match input {
            BitInput::CarryXorIo0 => self.carry ^ self.resolved_pins[0].unwrap_or(false),
            BitInput::CodecRx => self.registers[PINS] & (1 << 6) != 0,
            BitInput::Io0 => self.resolved_pins[0].unwrap_or(false),
            BitInput::Io1 => self.resolved_pins[1].unwrap_or(false),
        }
    }

    fn read_bit_endpoint(&self, endpoint: BitIo) -> bool {
        match endpoint {
            BitIo::Carry => self.carry,
            BitIo::CodecBit => self.registers[PINS] & (1 << 6) != 0,
            BitIo::Io0 => self.resolved_pins[0].unwrap_or(false),
            BitIo::Io1 => self.resolved_pins[1].unwrap_or(false),
        }
    }

    fn write_bit_endpoint(&mut self, endpoint: BitIo, value: bool) {
        match endpoint {
            BitIo::Carry => self.carry = value,
            BitIo::CodecBit => set_bit(&mut self.registers[CODEC_PERIOD], 7, value),
            BitIo::Io0 => set_bit(&mut self.registers[PINS], 0, value),
            BitIo::Io1 => set_bit(&mut self.registers[PINS], 1, value),
        }
    }

    fn wait_ready(&mut self, event: WaitEvent) -> bool {
        self.refresh_pin_inputs();
        match event {
            WaitEvent::CoreToHostClear => {
                self.registers[SYSTEM_CONTROL] & SYS_CORE_TO_HOST_PENDING == 0
            }
            WaitEvent::CodecDone => self.registers[CODEC_CONTROL] & (1 << 4) != 0,
            WaitEvent::Io0Falling => self.io0_falling,
            WaitEvent::Io0Rising => self.io0_rising,
            WaitEvent::HostToCorePending => {
                self.registers[SYSTEM_CONTROL] & SYS_HOST_TO_CORE_PENDING != 0
            }
            WaitEvent::Io1Changed => self.registers[PINS] & (1 << 3) != 0,
            WaitEvent::Io0Unchanged => self.registers[PINS] & (1 << 2) == 0,
            WaitEvent::Io0Changed => self.registers[PINS] & (1 << 2) != 0,
        }
    }

    fn consume_wait_event(&mut self, event: WaitEvent) {
        match event {
            WaitEvent::Io0Falling => self.io0_falling = false,
            WaitEvent::Io0Rising => self.io0_rising = false,
            _ => {}
        }
    }

    fn refresh_pin_inputs(&mut self) {
        self.refresh_pin_inputs_at(self.cycles);
    }

    fn refresh_pin_inputs_at(&mut self, cycle: u64) {
        let previous = self.resolved_pins;
        for (pin, previous_level) in previous.iter().copied().enumerate() {
            self.resolved_pins[pin] = self.resolve_pin(pin);
            if previous_level != self.resolved_pins[pin] {
                self.trace.push(TraceEvent {
                    cycle,
                    kind: TraceKind::Pin,
                    pc: Some(self.pc),
                    word: None,
                    address: Some(pin as u16),
                    value: self.resolved_pins[pin].map(u8::from),
                    detail: Some(if self.pin_conflict(pin) {
                        "resolved_x".into()
                    } else {
                        "resolved".into()
                    }),
                });
                if self.registers[TIMER_CONTROL] & (1 << (6 + pin)) != 0 {
                    self.registers[SYSTEM_CONTROL] |= SYS_INTERRUPT_REQUEST;
                    if self.state == RunState::Sleeping {
                        self.state = RunState::Running;
                    }
                }
            }
        }
        self.io0_rising |= previous[0] == Some(false) && self.resolved_pins[0] == Some(true);
        self.io0_falling |= previous[0] == Some(true) && self.resolved_pins[0] == Some(false);

        let input0 = self.resolved_pins[0].unwrap_or(false);
        let input1 = self.resolved_pins[1].unwrap_or(false);
        set_bit(&mut self.registers[PINS], 4, input0);
        set_bit(&mut self.registers[PINS], 5, input1);
        let xor0 = input0 ^ (self.registers[PINS] & 1 != 0);
        let xor1 = input1 ^ (self.registers[PINS] & 2 != 0);
        set_bit(&mut self.registers[PINS], 2, xor0);
        set_bit(&mut self.registers[PINS], 3, xor1);
        set_bit(&mut self.registers[PINS], 7, input0 ^ input1);
    }

    fn resolve_pin(&self, pin: usize) -> Option<bool> {
        let core = self.core_pin_outputs[pin];
        let external = self.external_pins[pin];
        match (core, external) {
            (Some(core), Some(external)) if core != external => None,
            (Some(value), _) | (_, Some(value)) => Some(value),
            (None, None) if self.registers[PIN_DIRECTION] & (1 << (pin + 2)) != 0 => Some(true),
            (None, None) => None,
        }
    }

    fn pin_conflict(&self, pin: usize) -> bool {
        matches!(
            (self.core_pin_outputs[pin], self.external_pins[pin]),
            (Some(core), Some(external)) if core != external
        )
    }

    fn sync_status(&mut self) {
        set_bit(&mut self.registers[STATUS], 0, self.carry);
        set_bit(&mut self.registers[STATUS], 2, self.zero);
        if self.stack.is_empty() {
            self.registers[STATUS] &= !STATUS_STACK_USED;
        } else {
            self.registers[STATUS] |= STATUS_STACK_USED;
        }
    }

    fn record_core_pin_changes(&mut self, cycle: u64) {
        let direction = self.registers[PIN_DIRECTION];
        let output = self.registers[PINS];
        for pin in 0..2 {
            let claimed = self.registers[SYSTEM_CONTROL] & (SYS_HOST_CLAIM_IO0 << pin) != 0;
            let next = if claimed && direction & (1 << pin) != 0 {
                if pin == 0
                    && self.codec_active
                    && self.registers[CODEC_CONTROL] & CODEC_TX_ENABLE != 0
                {
                    Some(self.codec_output)
                } else if pin == 0 && self.registers[TIMER_CONTROL] & TIMER_OUTPUT_ENABLE != 0 {
                    Some(self.timer_pin_level())
                } else {
                    Some(output & (1 << pin) != 0)
                }
            } else {
                None
            };
            if self.core_pin_outputs[pin] != next {
                self.core_pin_outputs[pin] = next;
                self.trace.push(TraceEvent {
                    cycle,
                    kind: TraceKind::Pin,
                    pc: Some(self.pc),
                    word: None,
                    address: Some(pin as u16),
                    value: next.map(u8::from),
                    detail: Some("core".into()),
                });
            }
        }
        self.refresh_pin_inputs_at(cycle);
    }

    fn advance_peripherals(&mut self, cycles: u8) -> (u8, bool) {
        for offset in 1..=cycles {
            self.tick_timer();
            self.tick_codec();
            let event_cycle = self.cycles + u64::from(offset);
            if self.reset_requested {
                self.reset_core(event_cycle, "wait_timeout_reset");
                self.record_core_pin_changes(event_cycle);
                return (offset, true);
            }
            self.record_core_pin_changes(event_cycle);
        }
        (cycles, false)
    }

    pub(crate) fn idle_until(&mut self, target_cycle: u64) {
        while self.cycles < target_cycle {
            let remaining = target_cycle - self.cycles;
            let chunk = remaining.min(u64::from(u8::MAX)) as u8;
            let (elapsed, reset) = self.advance_peripherals(chunk);
            self.cycles += u64::from(elapsed);
            if reset {
                return;
            }
        }
    }

    pub(crate) fn advance_gated_time(&mut self, target_cycle: u64) {
        self.cycles = self.cycles.max(target_cycle);
    }

    fn tick_timer(&mut self) {
        let control = self.registers[TIMER_CONTROL];
        let timeout = self.timeout_timer_active();
        if control & TIMER_ENABLE == 0 && !timeout {
            return;
        }
        const DIVIDERS: [u16; 8] = [1024, 256, 64, 16, 8, 4, 2, 1];
        self.timer_divider_phase += 1;
        if self.timer_divider_phase < DIVIDERS[usize::from(control & 7)] {
            return;
        }
        self.timer_divider_phase = 0;

        let count = self.registers[TIMER_COUNT];
        if control & TIMER_PWM_MODE != 0 {
            self.registers[TIMER_COUNT] = count.wrapping_add(1);
        } else if count == u8::MAX {
            self.registers[TIMER_COUNT] = self.registers[TIMER_RELOAD];
            if timeout {
                self.reset_requested = true;
            } else {
                self.timer_output = !self.timer_output;
            }
        } else {
            self.registers[TIMER_COUNT] = count + 1;
        }
    }

    fn timer_pin_level(&self) -> bool {
        if self.registers[TIMER_CONTROL] & TIMER_PWM_MODE != 0 {
            self.registers[TIMER_COUNT] > self.registers[TIMER_RELOAD]
        } else {
            self.timer_output
        }
    }

    fn tick_codec(&mut self) {
        let period = self.registers[CODEC_PERIOD] & 0x7f;
        let control = self.registers[CODEC_CONTROL];
        let enabled = period >= 4;
        if !enabled {
            self.codec_active = false;
            self.registers[CODEC_CONTROL] = (control & 0xe0) | CODEC_TAIL;
            return;
        }

        if !self.codec_active {
            self.codec_active = true;
            self.codec_phase = 0;
            self.codec_latched_bit = self.registers[CODEC_PERIOD] & 0x80 != 0;
            self.codec_input_start = self.resolved_pins[0].unwrap_or(false);
        } else {
            self.codec_phase += 1;
            if self.codec_phase >= period {
                self.codec_phase = 0;
                self.codec_latched_bit = self.registers[CODEC_PERIOD] & 0x80 != 0;
                self.codec_input_start = self.resolved_pins[0].unwrap_or(false);
            }
        }

        let quarter = u16::from(period).div_ceil(4) as u8;
        if control & CODEC_TX_ENABLE != 0 {
            self.codec_output = if control & CODEC_MANCHESTER != 0 {
                if self.codec_latched_bit {
                    self.codec_phase < period / 2
                } else {
                    false
                }
            } else {
                let high_cycles = if self.codec_latched_bit {
                    quarter.saturating_mul(3)
                } else {
                    quarter
                };
                self.codec_phase < high_cycles
            };
        } else if self.codec_phase == period / 2 {
            let input = self.resolved_pins[0].unwrap_or(false);
            let decoded = if control & CODEC_MANCHESTER != 0 {
                input != self.codec_input_start
            } else {
                input
            };
            set_bit(&mut self.registers[PINS], 6, decoded);
        }

        let tail = self.codec_phase >= quarter.saturating_mul(3);
        self.registers[CODEC_CONTROL] =
            (control & 0xe0) | if tail { CODEC_TAIL } else { 0 } | ((self.codec_phase >> 3) & 0x0f);
    }

    pub(crate) fn waiting_can_advance_internally(&self) -> bool {
        matches!(self.state, RunState::Waiting { event: 1..=3 })
            && self.registers[CODEC_PERIOD] & 0x7f >= 4
            || self.timeout_timer_active()
    }

    fn timeout_timer_active(&self) -> bool {
        matches!(self.state, RunState::Waiting { .. })
            && self.registers[STATUS] & STATUS_TIMEOUT_RESET != 0
            && self.registers[TIMER_CONTROL] & TIMER_ENABLE == 0
    }

    fn reset_core(&mut self, cycle: u64, reason: &str) {
        let host_control = self.registers[SYSTEM_CONTROL] & 0x1f;
        self.pc = RESET_PC;
        self.accumulator = 0;
        self.carry = false;
        self.zero = false;
        self.state = if host_control & SYS_HOST_CLOCK_RUN != 0 {
            RunState::Running
        } else {
            RunState::ClockGated
        };
        self.registers = [0; 0x400];
        self.registers[CODEC_CONTROL] = CODEC_TAIL;
        self.registers[SYSTEM_CONTROL] = host_control;
        self.indirect_address0 = 0;
        self.indirect_address1 = 0;
        self.stack.clear();
        self.timer_divider_phase = 0;
        self.timer_output = false;
        self.codec_phase = 0;
        self.codec_latched_bit = false;
        self.codec_input_start = false;
        self.codec_output = false;
        self.codec_active = false;
        self.reset_requested = false;
        self.trace.push(TraceEvent {
            cycle,
            kind: TraceKind::State,
            pc: Some(RESET_PC),
            word: None,
            address: None,
            value: None,
            detail: Some(reason.into()),
        });
    }

    fn load_flags_from_status(&mut self) {
        self.carry = self.registers[STATUS] & STATUS_CARRY != 0;
        self.zero = self.registers[STATUS] & STATUS_ZERO != 0;
    }

    fn push(&mut self, entry: StackEntry) -> Result<(), VmError> {
        if self.stack.len() == 6 {
            return Err(VmError::StackOverflow);
        }
        self.stack.push(entry);
        Ok(())
    }

    fn pop(&mut self) -> Result<StackEntry, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }

    fn pop_return(&mut self) -> Result<u16, VmError> {
        match self.pop()? {
            StackEntry::Return(address) => Ok(address),
            _ => Err(VmError::StackType {
                operation: "return",
            }),
        }
    }
}

fn set_bit(value: &mut u8, bit: u8, set: bool) {
    if set {
        *value |= 1 << bit;
    } else {
        *value &= !(1 << bit);
    }
}

fn branch(condition: bool, target: u16, mask: u16, pc: &mut u16, cycles: &mut u8) {
    if condition {
        *pc = (*pc & !mask) | (target & mask);
        *cycles = 2;
    }
}

fn writes_program_counter_low(instruction: Instruction) -> bool {
    use Instruction::*;
    match instruction {
        Clear(register) | StoreAccumulator(register) => register.address() == PCL as u16,
        Load {
            source,
            destination,
        }
        | Increment {
            source,
            destination,
        }
        | Decrement {
            source,
            destination,
        }
        | IncrementSkipZero {
            source,
            destination,
        }
        | DecrementSkipZero {
            source,
            destination,
        }
        | SwapNibbles {
            source,
            destination,
        }
        | And {
            source,
            destination,
        }
        | Or {
            source,
            destination,
        }
        | Xor {
            source,
            destination,
        }
        | Add {
            source,
            destination,
        }
        | Subtract {
            source,
            destination,
        }
        | RotateLeftCarry {
            source,
            destination,
        }
        | RotateRightCarry {
            source,
            destination,
        } => destination == Destination::Register && source.address() == PCL as u16,
        BitClear { register, .. } | BitSet { register, .. } => register.address() == PCL as u16,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(instructions: &[Instruction]) -> Machine {
        let mut words = vec![0];
        words.extend(
            instructions
                .iter()
                .map(|instruction| instruction.encode().unwrap()),
        );
        Machine::new(&words).unwrap()
    }

    #[test]
    fn arithmetic_flags_and_subtraction_direction_match_manual() {
        let mut vm = machine(&[
            Instruction::MoveLiteral(10),
            Instruction::SubtractLiteral(20),
            Instruction::CompareSubtractLiteral(5),
        ]);
        vm.step().unwrap();
        vm.step().unwrap();
        assert_eq!(vm.accumulator, 10);
        assert!(!vm.carry);
        vm.step().unwrap();
        assert!(vm.carry, "5 - 10 borrows");
    }

    #[test]
    fn call_and_return_are_two_cycles() {
        let mut words = vec![0, 0x7003, 0x0000, 0x0030];
        let mut vm = Machine::new(&words).unwrap();
        assert_eq!(vm.step().unwrap().cycles, 2);
        assert_eq!(vm.pc, 3);
        assert_eq!(vm.step().unwrap().cycles, 2);
        assert_eq!(vm.pc, 2);
        words.clear();
    }

    #[test]
    fn wait_stalls_until_host_event() {
        let mut vm = machine(&[Instruction::Wait(WaitEvent::HostToCorePending)]);
        let first = vm.step().unwrap();
        assert_eq!(first.state, RunState::Waiting { event: 4 });
        assert_eq!(vm.step().unwrap().cycles, 0);
        vm.host_write(COMMAND_IN as u8, 0x55);
        assert_eq!(vm.step().unwrap().state, RunState::Running);
        assert_eq!(vm.pc, 2);
    }

    #[test]
    fn host_handshake_side_effects_are_visible() {
        let mut vm = machine(&[Instruction::StoreAccumulator(Register::low(
            COMMAND_OUT as u8,
        ))]);
        vm.accumulator = 0xa5;
        vm.step().unwrap();
        assert_ne!(vm.registers[SYSTEM_CONTROL] & SYS_CORE_TO_HOST_PENDING, 0);
        assert_eq!(vm.host_read(COMMAND_OUT as u8), 0xa5);
        assert_eq!(vm.registers[SYSTEM_CONTROL] & SYS_CORE_TO_HOST_PENDING, 0);
    }

    #[test]
    fn timer_honors_divider_and_reloads() {
        let mut vm = machine(&[Instruction::Nop, Instruction::Nop]);
        vm.host_write(TIMER_COUNT as u8, 0xff);
        vm.host_write(TIMER_RELOAD as u8, 0x80);
        vm.host_write(TIMER_CONTROL as u8, TIMER_ENABLE | 0x06);
        vm.step().unwrap();
        assert_eq!(vm.registers[TIMER_COUNT], 0xff);
        vm.step().unwrap();
        assert_eq!(vm.registers[TIMER_COUNT], 0x80);
        assert!(vm.timer_output);
    }

    #[test]
    fn timer_pwm_drives_io0_and_emits_pin_events() {
        let mut vm = machine(&[Instruction::Nop, Instruction::Nop]);
        vm.host_write(PIN_DIRECTION as u8, 1);
        vm.host_write(TIMER_COUNT as u8, 0xfc);
        vm.host_write(TIMER_RELOAD as u8, 0xfd);
        vm.host_write(
            TIMER_CONTROL as u8,
            TIMER_ENABLE | TIMER_OUTPUT_ENABLE | TIMER_PWM_MODE | 7,
        );
        vm.step().unwrap();
        vm.step().unwrap();
        assert!(vm.trace.iter().any(|event| {
            event.kind == TraceKind::Pin
                && event.address == Some(0)
                && event.value == Some(1)
                && event.cycle == 2
        }));
    }

    #[test]
    fn codec_pwm_emits_quarter_and_three_quarter_edges() {
        let instructions = vec![Instruction::Nop; 9];
        let mut vm = machine(&instructions);
        vm.host_write(PIN_DIRECTION as u8, 1);
        vm.host_write(CODEC_PERIOD as u8, 8);
        vm.host_write(CODEC_CONTROL as u8, CODEC_TX_ENABLE);
        vm.run_steps(9).unwrap();

        let changes = vm
            .trace
            .iter()
            .filter(|event| {
                event.kind == TraceKind::Pin
                    && event.address == Some(0)
                    && event.detail.as_deref() == Some("core")
            })
            .map(|event| (event.cycle, event.value))
            .collect::<Vec<_>>();
        assert_eq!(changes, vec![(1, Some(1)), (3, Some(0)), (9, Some(1))]);
    }

    #[test]
    fn codec_pwm_receive_samples_the_midpoint() {
        let instructions = vec![Instruction::Nop; 5];
        let mut vm = machine(&instructions);
        vm.set_external_pin(0, true);
        vm.host_write(CODEC_PERIOD as u8, 8);
        vm.host_write(CODEC_CONTROL as u8, 0);
        vm.run_steps(5).unwrap();
        assert_ne!(vm.registers[PINS] & (1 << 6), 0);

        let mut low = machine(&instructions);
        low.set_external_pin(0, false);
        low.host_write(CODEC_PERIOD as u8, 8);
        low.host_write(CODEC_CONTROL as u8, 0);
        low.run_steps(5).unwrap();
        assert_eq!(low.registers[PINS] & (1 << 6), 0);
    }

    #[test]
    fn indirect_addresses_preserve_page_bits_and_post_increment() {
        let mut vm = machine(&[
            Instruction::MoveLiteralIndirect0(0xaa),
            Instruction::MoveLiteralIndirect1(0x55),
            Instruction::MoveLiteralIndirect1(0x66),
        ]);
        // Page bits are target-specific state restored by PUSHA2/POPA2. The
        // CH32X035 reuses the generic high-page MOVIP/MOVIA encodings.
        vm.set_indirect0(0x123);
        vm.set_indirect1(0x2ff);
        vm.run_steps(3).unwrap();
        assert_eq!(vm.indirect_address0, 0x123);
        assert_eq!(vm.registers[INDIRECT_ADDRESS0], 0x23);
        assert_eq!(vm.registers[0x123], 0xaa);
        assert_eq!(vm.indirect_address1, 0x301);
        assert_eq!(vm.registers[INDIRECT_ADDRESS1], 0x01);
        assert_eq!(vm.registers[0x2ff], 0x55);
        assert_eq!(vm.registers[0x300], 0x66);
    }

    #[test]
    fn indirect_data1_only_post_increments_for_mov_and_mova() {
        let port = Register::low(INDIRECT_DATA1 as u8);
        let mut vm = machine(&[
            Instruction::MoveIndirect1(0x20),
            Instruction::BitTestSkipSet {
                register: port,
                bit: 7,
            },
            Instruction::Load {
                source: port,
                destination: Destination::Accumulator,
            },
            Instruction::MoveLiteral(0x5a),
            Instruction::StoreAccumulator(port),
        ]);
        vm.registers[0x20] = 0x12;

        vm.step().unwrap();
        vm.step().unwrap();
        assert_eq!(vm.indirect_address1, 0x20, "bit tests must not advance p1");
        vm.step().unwrap();
        assert_eq!(vm.accumulator, 0x12);
        assert_eq!(vm.indirect_address1, 0x21, "MOV idata1,A advances p1");
        vm.step().unwrap();
        vm.step().unwrap();
        assert_eq!(vm.registers[0x21], 0x5a);
        assert_eq!(vm.indirect_address1, 0x22, "MOVA idata1 advances p1");
    }

    #[test]
    fn wait_timeout_uses_timer_then_resets_the_core() {
        let mut vm = machine(&[Instruction::Wait(WaitEvent::HostToCorePending)]);
        vm.host_write(STATUS as u8, STATUS_TIMEOUT_RESET);
        vm.host_write(TIMER_COUNT as u8, 0xfe);
        vm.host_write(TIMER_RELOAD as u8, 0xfe);
        vm.host_write(TIMER_CONTROL as u8, 7);

        assert_eq!(vm.step().unwrap().state, RunState::Waiting { event: 4 });
        assert_eq!(vm.registers[TIMER_COUNT], 0xff);
        vm.idle_until(10);
        assert_eq!(vm.pc, RESET_PC);
        assert_eq!(vm.state, RunState::Running);
        assert_eq!(vm.cycles, 2);
        assert!(vm.trace.iter().any(|event| {
            event.kind == TraceKind::State
                && event.cycle == 2
                && event.detail.as_deref() == Some("wait_timeout_reset")
        }));
    }

    #[test]
    fn gpio_resolves_release_pullup_and_drive_conflict() {
        let mut vm = machine(&[Instruction::Nop, Instruction::Nop]);
        vm.host_write(PIN_DIRECTION as u8, 1 << 2);
        vm.step().unwrap();
        assert_eq!(vm.pin_level(0), Some(true));

        vm.set_external_pin(0, false);
        assert_eq!(vm.pin_level(0), Some(false));
        vm.set_external_drive(0, None);
        assert_eq!(vm.pin_level(0), Some(true));

        vm.set_external_pin(0, false);
        vm.host_write(PIN_DIRECTION as u8, 1);
        vm.host_write(PINS as u8, 1);
        vm.step().unwrap();
        assert_eq!(vm.pin_level(0), None, "opposing push-pull drives resolve X");
    }

    #[test]
    fn every_register_write_to_pcl_is_multicycle() {
        let pcl = Register::low(PCL as u8);
        for instruction in [
            Instruction::Increment {
                source: pcl,
                destination: Destination::Register,
            },
            Instruction::RotateLeftCarry {
                source: pcl,
                destination: Destination::Register,
            },
            Instruction::BitSet {
                register: pcl,
                bit: 0,
            },
        ] {
            let mut vm = machine(&[instruction]);
            assert_eq!(vm.step().unwrap().cycles, 2, "{instruction:?}");
        }
    }

    #[test]
    fn host_clock_gate_reset_and_pin_ownership_are_effective() {
        let mut vm = machine(&[Instruction::Nop]);
        vm.host_write(PIN_DIRECTION as u8, 1);
        vm.host_write(PINS as u8, 1);
        vm.step().unwrap();
        assert_eq!(vm.core_pin_outputs[0], Some(true));

        vm.host_write(SYSTEM_CONTROL as u8, SYS_HOST_CLOCK_RUN);
        assert_eq!(vm.core_pin_outputs[0], None, "host reclaimed IO0");
        vm.host_write(SYSTEM_CONTROL as u8, 0);
        assert_eq!(vm.state, RunState::ClockGated);
        assert_eq!(vm.step().unwrap().cycles, 0);

        vm.host_write(
            SYSTEM_CONTROL as u8,
            SYS_HOST_CLOCK_RUN | SYS_HOST_CLAIM_IO0 | SYS_HOST_CLAIM_IO1 | SYS_HOST_RESET,
        );
        assert_eq!(vm.pc, RESET_PC);
        assert_eq!(vm.accumulator, 0);
        assert_eq!(vm.state, RunState::ResetHeld);
        vm.host_write(
            SYSTEM_CONTROL as u8,
            SYS_HOST_CLOCK_RUN | SYS_HOST_CLAIM_IO0 | SYS_HOST_CLAIM_IO1,
        );
        assert_eq!(vm.state, RunState::Running);
    }

    #[test]
    fn read_code_space_selectors_alias_the_loaded_image() {
        let mut words = vec![0; 0x11];
        words[1] = Instruction::ReadCode { space: 3 }.encode().unwrap();
        words[0x10] = 0xabcd;
        let mut vm = Machine::new(&words).unwrap();
        vm.set_indirect0(0x10);
        assert_eq!(vm.step().unwrap().cycles, 3);
        assert_eq!(vm.accumulator, 0xcd);
        assert_eq!(vm.indirect_address0 & 0xff, 0xab);
    }

    #[test]
    fn sleeping_core_only_wakes_on_enabled_pin_change() {
        let mut vm = machine(&[Instruction::Sleep { mode: 0 }, Instruction::Nop]);
        vm.set_external_pin(0, false);
        vm.step().unwrap();
        assert_eq!(vm.state, RunState::Sleeping);
        vm.set_external_pin(0, true);
        assert_eq!(vm.state, RunState::Sleeping);

        vm.host_write(TIMER_CONTROL as u8, 1 << 6);
        vm.set_external_pin(0, false);
        assert_eq!(vm.state, RunState::Running);
        assert_ne!(vm.registers[SYSTEM_CONTROL] & SYS_INTERRUPT_REQUEST, 0);
        vm.host_write(COMMAND_OUT as u8, 0);
        assert_eq!(vm.registers[SYSTEM_CONTROL] & SYS_INTERRUPT_REQUEST, 0);
    }
}
