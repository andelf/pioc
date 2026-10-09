# pioc

Rust tooling for the WCH CH32X035/CH643 Programmable Protocol I/O Controller
(PIOC/eMCU): an assembler, lossless disassembler, proc macros, cycle-aware VM,
and WaveDrom/VCD waveform exporters.

The implementation is based on the WCH PIOC manual, the vendor assembler and
the five official EVT examples. The legacy assembler currently reproduces all
five vendor `.BIN` files byte for byte. Canonical disassembly also round-trips
those binaries without loss.

## Workspace

| Crate | Purpose |
|---|---|
| `pioc-core` | `no_std` instruction, register, encoder and target-aware decoder model |
| `pioc-parser` | WCH legacy and normalized canonical assembly syntax |
| `pioc-proc` | `pioc_asm!`, `pioc_asm_canonical!` and `pioc_file!` compile-time assembly |
| `pioc-vm` | CPU, SFR, host handshake, GPIO, Timer0, scenarios and waveforms |
| `pioc-programs` | `no_std` Embassy-style WS2812/DS18x20 protocol APIs over hardware or VM backends |
| `pioc` | command-line assembler, disassembler and VM runner |

## CLI

```console
# WCH-compatible source to little-endian BIN
cargo run -p pioc -- assemble program.ASM -o program.bin

# Vendor /F-style complete 4096-byte image
cargo run -p pioc -- assemble --full program.ASM -o program-full.bin

# Normalized source
cargo run -p pioc -- assemble --canonical program.pio -o program.bin

# Convert WCH/EVT source while retaining labels and comments
cargo run -p pioc -- translate program.ASM -o program.pio

# Lossless normalized listing
cargo run -p pioc -- disassemble --canonical program.bin

# Reassemblable source rather than an annotated address listing
cargo run -p pioc -- disassemble --canonical --source program.bin -o program.pio

# VM trace plus waveform files
cargo run -p pioc -- run program.bin \
  --scenario scenario.json \
  --pin0 z --pin1 high \
  --trace trace.jsonl \
  --wavejson waveform.json \
  --vcd waveform.vcd
```

A scenario is a cycle-ordered JSON array. Addresses and values are decimal JSON
numbers; for example, write data register 0 and then send command `1`:

```json
[
  {"cycle": 10, "type": "host_write", "address": 32, "value": 165},
  {"cycle": 10, "type": "host_write", "address": 30, "value": 1},
  {"cycle": 200, "type": "pin", "pin": 0, "value": false},
  {"cycle": 300, "type": "pin", "pin": 0, "value": true},
  {"cycle": 400, "type": "pin", "pin": 0, "value": null}
]
```

Pin `null` releases the external driver; internal pull-ups and PIOC output
direction then determine the resolved level. Conflicting push-pull drives are
reported as unknown rather than silently choosing one side.

For a fixed cycle-0 input, `run` also accepts `--pin0` and `--pin1`. Each value
is `low`/`0`, `high`/`1`, or `z`/`release`. A cycle-0 scenario event is applied
after these initial values and can therefore override them. Use scenario pin
events for later input transitions.

The WaveJSON output can be pasted into WaveDrom. VCD can be opened by GTKWave
or another waveform viewer. `--wave-start`, `--wave-end` and `--wave-quantum`
select the displayed cycle window and sampling density; the default is two
PIOC cycles per WaveDrom slot. `--clock-hz` defaults
to the standard 48 MHz PIOC clock and controls both WaveDrom duration marks and
VCD timestamps. WaveDrom marks show each interval as `cycles / physical time`;
its footer records the physical duration represented by one displayed slot.

## Syntax and ISA

The parser accepts the vendor `INCLUDE`, `EQU`, `ORG`, `DW`, `DB` and `END`
directives and aliases used by the EVT sources. Canonical syntax deliberately
has a small core: familiar operations such as `mov`, `add`, `jmp`, `bset` and
`skip`, plus semantic register names such as `timer0.ctrl`, `pins.dir`, `cmd.in`,
`ptr0` and `d0`. `.alias` and `.const` are optional conveniences; rare forms
such as `mov.lit` are lossless-encoding escapes, not concepts required for
normal programs.

The five translated EVT programs under [`examples/canonical`](examples/canonical)
are assembled in tests and must reproduce the vendor BIN files byte for byte.

See [docs/isa.md](docs/isa.md) for the complete instruction map and canonical
syntax, and [docs/registers.md](docs/registers.md) for every SFR and bitfield.
The detailed high-level driver design and Embassy comparison are in
[docs/programs-api-design.md](docs/programs-api-design.md).

## VM scope

Implemented today: deterministic CPU instructions, six-entry stack, flags,
indirect registers, host command handshake, GPIO edge waits, digital `0/1/Z/X`
resolution with pull-ups, Timer0 divider/reload/PWM output, PWM/Manchester codec
TX and normal-waveform RX, WAIT timeout reset, scheduled host/pin events, trace
JSONL, WaveJSON and VCD.

Architectural boundary: the PIOC manual says program-space `WRCODE` is not
supported, and its generic `EXEC` opcode range is redefined as `BCTC` on the
CH32X035 target. Unknown raw words therefore stop rather than inventing an
operation. Exact codec receive phase, analog rise/fall behavior, and same-cycle
hardware priority remain calibration points. They are reported as limits rather
than silently presented as verified silicon behavior.

`Machine::new` starts in a standalone run/claim-both configuration; later
`sys.ctrl` host writes enforce pin ownership, reset hold, and clock gating.

## Verification

```console
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The integration tests locate the sibling WCH EVT tree when available and check
all official ASM/BIN pairs as golden files.

## Sources

- [WCH support thread containing the manual, assembler and examples](https://www.wch.cn/bbs/thread-111140-1.html)
- [Original `andelf/pioc` repository](https://github.com/andelf/pioc)

This project is an independent implementation; WCH product and instruction
names remain the property of their respective owner.
