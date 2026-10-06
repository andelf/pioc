//! Waveform exporters for VM trace events.

use serde_json::{json, Value};
use thiserror::Error;

use crate::{TraceEvent, TraceKind};

const MAX_SLOTS: u64 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaveOptions {
    pub start_cycle: u64,
    pub end_cycle: u64,
    pub quantum_cycles: u64,
    pub clock_hz: u64,
}

impl Default for WaveOptions {
    fn default() -> Self {
        Self {
            start_cycle: 0,
            end_cycle: 256,
            quantum_cycles: 1,
            clock_hz: 48_000_000,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WaveError {
    #[error("waveform end cycle must be greater than its start cycle")]
    EmptyWindow,
    #[error("waveform quantum must be at least one cycle")]
    ZeroQuantum,
    #[error("waveform needs {actual} slots; the limit is {maximum}")]
    TooManySlots { actual: u64, maximum: u64 },
    #[error("VCD clock frequency must be non-zero")]
    ZeroClock,
}

/// Convert pin events to WaveDrom-compatible WaveJSON.
pub fn to_wavejson(trace: &[TraceEvent], options: WaveOptions) -> Result<Value, WaveError> {
    if options.clock_hz == 0 {
        return Err(WaveError::ZeroClock);
    }
    let slots = slot_count(options)?;
    let mut lanes = [
        vec![None; slots],
        vec![None; slots],
        vec![None; slots],
        vec![None; slots],
        vec![None; slots],
        vec![None; slots],
    ];
    lanes[0][0] = Some('z');
    lanes[1][0] = Some('z');
    lanes[2][0] = Some('z');
    lanes[3][0] = Some('z');
    lanes[4][0] = Some('z');
    lanes[5][0] = Some('z');

    for event in trace {
        if event.kind != TraceKind::Pin || event.cycle >= options.end_cycle {
            continue;
        }
        let Some(pin @ 0..=1) = event.address else {
            continue;
        };
        let detail = event.detail.as_deref();
        let lane_group = match detail {
            Some("external") => 2,
            Some("resolved" | "resolved_x") => 4,
            _ => 0,
        };
        let lane = usize::from(pin) + lane_group;
        if event.cycle < options.start_cycle {
            lanes[lane][0] = Some(if detail == Some("resolved_x") {
                'x'
            } else {
                logic_char(event.value)
            });
            continue;
        }
        let slot = ((event.cycle - options.start_cycle) / options.quantum_cycles) as usize;
        lanes[lane][slot] = Some(if detail == Some("resolved_x") {
            'x'
        } else {
            logic_char(event.value)
        });
    }

    for lane in &mut lanes {
        propagate_and_compress(lane);
    }

    let (nodes, edges) = duration_annotations(trace, options, slots);
    let slot_time = duration_label(options.quantum_cycles, options.clock_hz);
    let clock_mhz = options.clock_hz as f64 / 1_000_000.0;
    let clock_text = format!("{clock_mhz:.6}");
    let clock_text = clock_text.trim_end_matches('0').trim_end_matches('.');

    Ok(json!({
        "signal": [
            ["core", {"name": "io0", "wave": lane_string(&lanes[0])},
                     {"name": "io1", "wave": lane_string(&lanes[1])}],
            ["external", {"name": "io0", "wave": lane_string(&lanes[2])},
                         {"name": "io1", "wave": lane_string(&lanes[3])}],
            ["resolved", {"name": "io0", "wave": lane_string(&lanes[4]), "node": lane_string(&nodes[0])},
                         {"name": "io1", "wave": lane_string(&lanes[5]), "node": lane_string(&nodes[1])}]
        ],
        "edge": edges,
        "head": {
            "text": format!(
                "PIOC @ {clock_text} MHz; cycles {}..{}",
                options.start_cycle,
                options.end_cycle
            ),
            "tick": options.start_cycle
        },
        "foot": {
            "text": format!(
                "1 WaveDrom slot = {} PIOC cycle(s) = {slot_time}",
                options.quantum_cycles
            )
        },
        "config": {"hscale": 1},
        "_pioc": {
            "clock_hz": options.clock_hz,
            "start_cycle": options.start_cycle,
            "end_cycle": options.end_cycle,
            "quantum_cycles": options.quantum_cycles,
            "resolved_nodes": [lane_string(&nodes[0]), lane_string(&nodes[1])]
        }
    }))
}

fn duration_annotations(
    trace: &[TraceEvent],
    options: WaveOptions,
    slots: usize,
) -> ([Vec<Option<char>>; 2], Vec<String>) {
    let mut nodes = [vec![Some('.'); slots], vec![Some('.'); slots]];
    let mut edges = Vec::new();
    let markers: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
        .chars()
        .collect();
    let mut next_marker = 0;

    for pin in 0..=1_u16 {
        let mut previous: Option<(u64, usize, char)> = None;
        for event in trace.iter().filter(|event| {
            event.kind == TraceKind::Pin
                && event.address == Some(pin)
                && matches!(event.detail.as_deref(), Some("resolved" | "resolved_x"))
                && event.cycle >= options.start_cycle
                && event.cycle < options.end_cycle
        }) {
            let slot = ((event.cycle - options.start_cycle) / options.quantum_cycles) as usize;
            if previous.is_some_and(|(_, previous_slot, _)| previous_slot == slot) {
                continue;
            }
            let Some(&marker) = markers.get(next_marker) else {
                break;
            };
            next_marker += 1;
            nodes[usize::from(pin)][slot] = Some(marker);
            if let Some((previous_cycle, _, previous_marker)) = previous {
                let cycles = event.cycle - previous_cycle;
                edges.push(format!(
                    "{previous_marker}<->{marker} {} cyc / {}",
                    cycles,
                    duration_label(cycles, options.clock_hz)
                ));
            }
            previous = Some((event.cycle, slot, marker));
        }
        if let Some((previous_cycle, previous_slot, previous_marker)) = previous {
            let terminal_cycle = options.end_cycle - 1;
            let terminal_slot = slots - 1;
            if terminal_slot > previous_slot {
                if let Some(&marker) = markers.get(next_marker) {
                    next_marker += 1;
                    nodes[usize::from(pin)][terminal_slot] = Some(marker);
                    let cycles = terminal_cycle - previous_cycle;
                    edges.push(format!(
                        "{previous_marker}<->{marker} {} cyc / {}",
                        cycles,
                        duration_label(cycles, options.clock_hz)
                    ));
                }
            }
        }
    }
    (nodes, edges)
}

fn duration_label(cycles: u64, clock_hz: u64) -> String {
    let picoseconds = cycles as u128 * 1_000_000_000_000_u128 / clock_hz as u128;
    let (value, unit) = if picoseconds >= 1_000_000_000_000 {
        (picoseconds as f64 / 1_000_000_000_000.0, "s")
    } else if picoseconds >= 1_000_000_000 {
        (picoseconds as f64 / 1_000_000_000.0, "ms")
    } else if picoseconds >= 1_000_000 {
        (picoseconds as f64 / 1_000_000.0, "us")
    } else if picoseconds >= 1_000 {
        (picoseconds as f64 / 1_000.0, "ns")
    } else {
        return format!("{picoseconds} ps");
    };
    let text = format!("{value:.3}");
    format!(
        "{} {unit}",
        text.trim_end_matches('0').trim_end_matches('.')
    )
}

/// Convert core pin events to a compact VCD document.
pub fn to_vcd(trace: &[TraceEvent], clock_hz: u64) -> Result<String, WaveError> {
    if clock_hz == 0 {
        return Err(WaveError::ZeroClock);
    }

    let mut output = String::from(
        "$timescale 1ps $end\n$scope module pioc $end\n\
         $var wire 1 ! io0 $end\n$var wire 1 \" io1 $end\n\
         $upscope $end\n$enddefinitions $end\n#0\nz!\nz\"\n",
    );
    let mut last_time = 0;
    for event in trace.iter().filter(|event| {
        event.kind == TraceKind::Pin
            && matches!(event.detail.as_deref(), Some("resolved" | "resolved_x"))
    }) {
        let Some(pin @ 0..=1) = event.address else {
            continue;
        };
        let time = event.cycle.saturating_mul(1_000_000_000_000) / clock_hz;
        if time != last_time {
            output.push_str(&format!("#{time}\n"));
            last_time = time;
        }
        output.push(if event.detail.as_deref() == Some("resolved_x") {
            'x'
        } else {
            logic_char(event.value)
        });
        output.push(if pin == 0 { '!' } else { '"' });
        output.push('\n');
    }
    Ok(output)
}

fn slot_count(options: WaveOptions) -> Result<usize, WaveError> {
    if options.quantum_cycles == 0 {
        return Err(WaveError::ZeroQuantum);
    }
    if options.end_cycle <= options.start_cycle {
        return Err(WaveError::EmptyWindow);
    }
    let duration = options.end_cycle - options.start_cycle;
    let slots = duration.div_ceil(options.quantum_cycles);
    if slots > MAX_SLOTS {
        return Err(WaveError::TooManySlots {
            actual: slots,
            maximum: MAX_SLOTS,
        });
    }
    Ok(slots as usize)
}

fn logic_char(value: Option<u8>) -> char {
    match value {
        Some(0) => '0',
        Some(_) => '1',
        None => 'z',
    }
}

fn propagate_and_compress(lane: &mut [Option<char>]) {
    for index in 1..lane.len() {
        if lane[index].is_none() {
            lane[index] = lane[index - 1];
        }
    }
    for index in (1..lane.len()).rev() {
        if lane[index] == lane[index - 1] {
            lane[index] = Some('.');
        }
    }
}

fn lane_string(lane: &[Option<char>]) -> String {
    lane.iter().map(|value| value.unwrap_or('x')).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(cycle: u64, pin: u16, value: Option<u8>) -> TraceEvent {
        TraceEvent {
            cycle,
            kind: TraceKind::Pin,
            pc: None,
            word: None,
            address: Some(pin),
            value,
            detail: Some("resolved".into()),
        }
    }

    #[test]
    fn wavejson_compresses_stable_pin_levels() {
        let json = to_wavejson(
            &[pin(0, 0, None), pin(2, 0, Some(1))],
            WaveOptions {
                start_cycle: 0,
                end_cycle: 5,
                quantum_cycles: 1,
                clock_hz: 48_000_000,
            },
        )
        .unwrap();
        assert_eq!(json["signal"][2][1]["wave"], "z.1..");
        assert_eq!(
            json["foot"]["text"],
            "1 WaveDrom slot = 1 PIOC cycle(s) = 20.833 ns"
        );
    }

    #[test]
    fn wavejson_marks_resolved_edge_durations() {
        let json = to_wavejson(
            &[pin(0, 0, Some(1)), pin(42, 0, Some(0))],
            WaveOptions {
                start_cycle: 0,
                end_cycle: 50,
                quantum_cycles: 1,
                clock_hz: 48_000_000,
            },
        )
        .unwrap();
        assert_eq!(json["edge"][0], "A<->B 42 cyc / 875 ns");
    }

    #[test]
    fn wavejson_window_inherits_the_last_prior_level() {
        let json = to_wavejson(
            &[pin(2, 0, Some(1)), pin(12, 0, Some(0))],
            WaveOptions {
                start_cycle: 10,
                end_cycle: 15,
                quantum_cycles: 1,
                clock_hz: 48_000_000,
            },
        )
        .unwrap();
        assert_eq!(json["signal"][2][1]["wave"], "1.0..");
    }

    #[test]
    fn vcd_uses_clock_to_convert_cycles_to_picoseconds() {
        let vcd = to_vcd(&[pin(48, 0, Some(1))], 48_000_000).unwrap();
        assert!(vcd.contains("#1000000\n1!"));
    }
}
