//! Discrete host and pin events for repeatable VM scenarios.

use serde::{Deserialize, Serialize};

use crate::{Machine, RunState, VmError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventAction {
    HostWrite {
        address: u8,
        value: u8,
    },
    HostRead {
        address: u8,
    },
    /// External drive. `null` releases the pin to high impedance.
    Pin {
        pin: u8,
        value: Option<bool>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduledEvent {
    pub cycle: u64,
    #[serde(flatten)]
    pub action: EventAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    pub max_steps: usize,
    pub max_cycles: u64,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_steps: 100_000,
            max_cycles: 10_000_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunReport {
    pub executed_steps: usize,
    pub applied_events: usize,
    pub final_cycle: u64,
    pub final_state: RunState,
    pub pending_events: usize,
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    StepLimit,
    CycleLimit,
    Blocked,
}

/// Run until a limit is reached or the VM cannot progress without another event.
///
/// Events are required to be ordered by cycle. An event that lands during a
/// multi-cycle instruction is committed at the following instruction boundary.
pub fn run_scenario(
    machine: &mut Machine,
    events: &[ScheduledEvent],
    limits: RunLimits,
) -> Result<RunReport, VmError> {
    for pair in events.windows(2) {
        if pair[0].cycle > pair[1].cycle {
            return Err(VmError::EventOrder {
                previous: pair[0].cycle,
                cycle: pair[1].cycle,
            });
        }
    }
    let mut event_index = 0;
    let mut executed_steps = 0;

    while machine.cycles < limits.max_cycles && executed_steps < limits.max_steps {
        while let Some(event) = events
            .get(event_index)
            .filter(|event| event.cycle <= machine.cycles)
        {
            apply(machine, event.action)?;
            event_index += 1;
        }

        if !matches!(
            machine.state,
            RunState::Sleeping | RunState::ClockGated | RunState::ResetHeld
        ) {
            let outcome = machine.step()?;
            if outcome.cycles != 0 {
                executed_steps += 1;
                continue;
            }
        }

        let Some(next) = events.get(event_index) else {
            if machine.waiting_can_advance_internally() {
                machine.idle_until((machine.cycles + 1).min(limits.max_cycles));
                continue;
            }
            break;
        };
        if machine.waiting_can_advance_internally() && next.cycle > machine.cycles + 1 {
            machine.idle_until(machine.cycles + 1);
            continue;
        }
        let target = next.cycle.min(limits.max_cycles);
        if target <= machine.cycles {
            continue;
        }
        if matches!(machine.state, RunState::ClockGated | RunState::ResetHeld) {
            machine.advance_gated_time(target);
        } else {
            machine.idle_until(target);
        }
    }

    while let Some(event) = events
        .get(event_index)
        .filter(|event| event.cycle <= machine.cycles)
    {
        apply(machine, event.action)?;
        event_index += 1;
    }

    let stop_reason = if executed_steps >= limits.max_steps {
        StopReason::StepLimit
    } else if machine.cycles >= limits.max_cycles {
        StopReason::CycleLimit
    } else {
        StopReason::Blocked
    };
    Ok(RunReport {
        executed_steps,
        applied_events: event_index,
        final_cycle: machine.cycles,
        final_state: machine.state,
        pending_events: events.len() - event_index,
        stop_reason,
    })
}

fn apply(machine: &mut Machine, action: EventAction) -> Result<(), VmError> {
    match action {
        EventAction::HostWrite { address, value } => machine.host_write(address, value),
        EventAction::HostRead { address } => {
            machine.host_read(address);
        }
        EventAction::Pin { pin, value } => {
            if pin > 1 {
                return Err(VmError::InvalidPin { pin });
            }
            machine.set_external_drive(usize::from(pin), value);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use pioc_core::{Instruction, WaitEvent};

    use super::*;

    #[test]
    fn future_event_fast_forwards_a_waiting_machine() {
        let words = vec![
            0,
            Instruction::Wait(WaitEvent::HostToCorePending)
                .encode()
                .unwrap(),
            Instruction::Nop.encode().unwrap(),
        ];
        let mut machine = Machine::new(&words).unwrap();
        let report = run_scenario(
            &mut machine,
            &[ScheduledEvent {
                cycle: 100,
                action: EventAction::HostWrite {
                    address: 0x1e,
                    value: 0x55,
                },
            }],
            RunLimits {
                max_steps: 2,
                max_cycles: 200,
            },
        )
        .unwrap();
        assert_eq!(report.applied_events, 1);
        assert_eq!(report.executed_steps, 2);
        assert_eq!(report.final_cycle, 101);
        assert_eq!(report.stop_reason, StopReason::StepLimit);
        assert_eq!(machine.pc, 2);
    }

    #[test]
    fn clock_gated_machine_jumps_to_resume_event_without_ticking_core() {
        let words = vec![0, Instruction::Nop.encode().unwrap()];
        let mut machine = Machine::new(&words).unwrap();
        let report = run_scenario(
            &mut machine,
            &[
                ScheduledEvent {
                    cycle: 0,
                    action: EventAction::HostWrite {
                        address: 0x1c,
                        value: 0,
                    },
                },
                ScheduledEvent {
                    cycle: 100,
                    action: EventAction::HostWrite {
                        address: 0x1c,
                        value: 0x0d,
                    },
                },
            ],
            RunLimits {
                max_steps: 1,
                max_cycles: 200,
            },
        )
        .unwrap();
        assert_eq!(report.applied_events, 2);
        assert_eq!(report.executed_steps, 1);
        assert_eq!(report.final_cycle, 101);
    }

    #[test]
    fn reports_blocked_wait_and_rejects_invalid_pin() {
        let words = vec![
            0,
            Instruction::Wait(WaitEvent::HostToCorePending)
                .encode()
                .unwrap(),
        ];
        let mut machine = Machine::new(&words).unwrap();
        let report = run_scenario(&mut machine, &[], RunLimits::default()).unwrap();
        assert_eq!(report.stop_reason, StopReason::Blocked);

        let mut machine = Machine::new(&words).unwrap();
        let error = run_scenario(
            &mut machine,
            &[ScheduledEvent {
                cycle: 0,
                action: EventAction::Pin {
                    pin: 2,
                    value: Some(true),
                },
            }],
            RunLimits::default(),
        )
        .unwrap_err();
        assert_eq!(error, VmError::InvalidPin { pin: 2 });
    }
}
