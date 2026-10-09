use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use pioc_core::{DecodeTarget, PROGRAM_WORDS};
use pioc_parser::{
    assemble_file_with_syntax, format_decoded, translate_legacy_file, Program, Syntax,
};
use pioc_vm::{run_scenario, to_vcd, to_wavejson, Machine, RunLimits, ScheduledEvent, WaveOptions};

#[derive(Debug, Parser)]
#[command(name = "pioc", about = "PIOC assembler, disassembler, and VM runner")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PinState {
    #[value(alias = "0")]
    Low,
    #[value(alias = "1")]
    High,
    #[value(alias = "release", alias = "hi-z")]
    Z,
}

impl PinState {
    fn drive(self) -> Option<bool> {
        match self {
            Self::Low => Some(false),
            Self::High => Some(true),
            Self::Z => None,
        }
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Assemble a PIOC source file.
    Assemble {
        input: PathBuf,
        /// Output BIN path. Defaults to the input path with a .bin extension.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Parse the normalized canonical syntax instead of WCH legacy syntax.
        #[arg(long)]
        canonical: bool,
        /// Pad the BIN to the complete 2048-word PIOC program space.
        #[arg(long)]
        full: bool,
    },
    /// Translate WCH legacy source into label-preserving canonical source.
    Translate {
        input: PathBuf,
        /// Output source path. Defaults to the input path with a .pio extension.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Decode a little-endian PIOC binary.
    Disassemble {
        input: PathBuf,
        /// Print canonical syntax instead of WCH-compatible legacy syntax.
        #[arg(long)]
        canonical: bool,
        /// Emit reassemblable source without addresses or machine words.
        #[arg(long)]
        source: bool,
        /// Write output to a file instead of stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Execute a little-endian PIOC binary in the cycle-aware VM.
    Run {
        input: PathBuf,
        /// Maximum number of instructions to execute.
        #[arg(long, default_value_t = 100_000)]
        max_steps: usize,
        /// Maximum simulated PIOC clock cycles.
        #[arg(long, default_value_t = 10_000_000)]
        max_cycles: u64,
        /// JSON array of scheduled host writes/reads and external pin changes.
        #[arg(long)]
        scenario: Option<PathBuf>,
        /// Write all VM events as newline-delimited JSON.
        #[arg(long)]
        trace: Option<PathBuf>,
        /// Write GPIO activity as WaveDrom-compatible WaveJSON.
        #[arg(long)]
        wavejson: Option<PathBuf>,
        /// Write core GPIO activity as Value Change Dump data.
        #[arg(long)]
        vcd: Option<PathBuf>,
        /// PIOC clock used for WaveDrom duration marks and VCD timestamps.
        #[arg(long, default_value_t = 48_000_000)]
        clock_hz: u64,
        /// External IO0 state at cycle 0: low/0, high/1, or z/release.
        #[arg(long)]
        pin0: Option<PinState>,
        /// External IO1 state at cycle 0: low/0, high/1, or z/release.
        #[arg(long)]
        pin1: Option<PinState>,
        /// First cycle included in WaveJSON.
        #[arg(long, default_value_t = 0)]
        wave_start: u64,
        /// Last cycle (exclusive) included in WaveJSON; defaults to VM completion.
        #[arg(long)]
        wave_end: Option<u64>,
        /// Number of PIOC cycles represented by one WaveJSON character.
        #[arg(long, default_value_t = 2)]
        wave_quantum: u64,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    match args.command {
        Command::Assemble {
            input,
            output,
            canonical,
            full,
        } => {
            let syntax = if canonical {
                Syntax::Canonical
            } else {
                Syntax::Legacy
            };
            let mut program = assemble_file_with_syntax(&input, syntax)?;
            if full {
                program.words.resize(PROGRAM_WORDS, 0);
            }
            let output = output.unwrap_or_else(|| input.with_extension("bin"));
            std::fs::write(output, program.to_le_bytes())?;
        }
        Command::Translate { input, output } => {
            let source = translate_legacy_file(&input)?;
            let output = output.unwrap_or_else(|| input.with_extension("pio"));
            std::fs::write(output, source)?;
        }
        Command::Disassemble {
            input,
            canonical,
            source,
            output,
        } => {
            let bytes = std::fs::read(input)?;
            let program = Program::from_le_bytes(&bytes)?;
            let syntax = if canonical {
                Syntax::Canonical
            } else {
                Syntax::Legacy
            };
            let mut text = String::new();
            if source {
                text.push_str(if canonical { ".origin 0\n" } else { "ORG 0\n" });
            }
            for (pc, decoded) in program
                .decode(DecodeTarget::Ch32x035Pioc)
                .iter()
                .enumerate()
            {
                let instruction = format_decoded(*decoded, syntax);
                if source {
                    text.push_str(&instruction);
                } else {
                    text.push_str(&format!("{pc:04x}: {:04x}  {instruction}", decoded.word));
                }
                text.push('\n');
            }
            if !canonical && source {
                text.push_str("END\n");
            }
            if let Some(path) = output {
                std::fs::write(path, text)?;
            } else {
                print!("{text}");
            }
        }
        Command::Run {
            input,
            max_steps,
            max_cycles,
            scenario,
            trace,
            wavejson,
            vcd,
            clock_hz,
            pin0,
            pin1,
            wave_start,
            wave_end,
            wave_quantum,
        } => {
            let bytes = std::fs::read(input)?;
            let program = Program::from_le_bytes(&bytes)?;
            let mut machine = Machine::new(&program.words)?;
            if let Some(state) = pin0 {
                machine.set_external_drive(0, state.drive());
            }
            if let Some(state) = pin1 {
                machine.set_external_drive(1, state.drive());
            }
            let events: Vec<ScheduledEvent> = match scenario {
                Some(path) => serde_json::from_slice(&std::fs::read(path)?)?,
                None => Vec::new(),
            };
            let report = run_scenario(
                &mut machine,
                &events,
                RunLimits {
                    max_steps,
                    max_cycles,
                },
            )?;

            if let Some(path) = trace {
                let mut jsonl = String::new();
                for event in machine.trace() {
                    jsonl.push_str(&serde_json::to_string(event)?);
                    jsonl.push('\n');
                }
                std::fs::write(path, jsonl)?;
            }
            if let Some(path) = wavejson {
                let end_cycle = wave_end.unwrap_or_else(|| machine.cycles.max(wave_start + 1));
                let document = to_wavejson(
                    machine.trace(),
                    WaveOptions {
                        start_cycle: wave_start,
                        end_cycle,
                        quantum_cycles: wave_quantum,
                        clock_hz,
                    },
                )?;
                std::fs::write(path, serde_json::to_vec_pretty(&document)?)?;
            }
            if let Some(path) = vcd {
                std::fs::write(path, to_vcd(machine.trace(), clock_hz)?)?;
            }

            println!(
                "executed {} instructions in {} cycles; events={}/{}, stop={:?}, state={:?}, pc=0x{:03x}",
                report.executed_steps,
                machine.cycles,
                report.applied_events,
                events.len(),
                report.stop_reason,
                machine.state,
                machine.pc
            );
        }
    }
    Ok(())
}
