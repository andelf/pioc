# pioc

Rust tooling for the WCH Programmable Protocol I/O Controller (PIOC, also
called the eMCU): an assembler, lossless disassembler, compile-time proc
macros, a cycle-aware virtual machine, waveform exporters, and `no_std`
protocol drivers that run on the chip itself.

The implementation is based on the WCH PIOC manual, the vendor assembler and
the official EVT examples. The legacy assembler reproduces the vendor `.BIN`
files byte for byte, and canonical disassembly round-trips them without loss.

It is the firmware and tooling half of the
[`ch32-hal`](https://github.com/ch32-rs/ch32-hal) PIOC support: `ch32-hal`
provides the host-side register driver, this workspace provides the images it
loads and the drivers that speak their command set.

## PIOC hardware

The block appears in these WCH families, according to the
[`ch32-data`](https://github.com/ch32-rs/ch32-data) peripheral database:

| Family | Chips | Register block | Program SRAM | IRQ |
|---|---|---|---|---|
| CH32X0 | CH32X033/034/035 | `0x4002_6C00`; IO0/IO1 on PC18/PC19, IO0 remap on PC7 | `0x2000_4000`, the top 4 KiB of the 20 KiB SRAM | 47 |
| CH643 | CH643 | same block and pins as CH32X0 | `0x2000_4000` (inferred from X0, not yet checked against the vendor header) | 47 |
| CH32V205 | CH32V205CCT6/RCT6/VCT6 | `0x4002_5C00`; IO0 on PA11/PB10/PD3/PE3, IO1 on PA12/PB11/PD4/PE4, all AF4 | `0x2000_0000`, the *low* 4 KiB, so the vendor linker moves `RAM` to `0x2000_1000` | 63 |
| CH32H4 | CH32H415/416/417 | `0x4002_5C00`; IO0 on PB8/PC1/PE3/PF12, IO1 on PB9/PC0/PC2/PE4/PF13/PF14 | `0x5004_0000` | 122 |

The instruction model and VM target the CH32X035 revision of the core
(`DecodeTarget::Ch32x035Pioc`). CH32X035 and CH32H417 are validated on
hardware; CH32V205 has a different firmware ABI (below) and has not been
exercised.

Two firmware ABIs exist for the vendor 1-Wire/RGB example:

| | CH32H417, CH32X035 | CH32V205 |
|---|---|---|
| RAM command | `0x80 \| bit_cycles` | `0x70` |
| Payload offset | `0x400` | `0x600` |
| Capacity | 3072 bytes | 2560 bytes |

`pioc-programs` models the first one; the second is a planned addition.

## Workspace

| Crate | Purpose |
|---|---|
| `pioc-core` | `no_std` instruction, register, encoder and target-aware decoder model |
| `pioc-parser` | WCH legacy and normalized canonical assembly syntax |
| `pioc-proc` | `pioc_asm!`, `pioc_asm_canonical!`, `pioc_file!` and `pioc_file_canonical!` compile-time assembly |
| `pioc-vm` | CPU, SFR, host handshake, GPIO, Timer0, codec, scenarios and waveforms |
| `pioc-programs` | `no_std` Embassy-style WS2812/DS18x20 drivers over a backend-neutral `CommandPort` |
| `pioc` | command-line assembler, disassembler and VM runner |

## Using it from firmware

`pioc_file_canonical!` assembles checked-in firmware source into a
`&'static [u16]` image at build time, and `Firmware` carries the assumptions
baked into that image:

```rust
use pioc_programs::ws2812::{bit_cycles_for_hz, ColorOrder, Config, OutputPin, Storage, Ws2812};
use pioc_programs::Firmware;

const FIRMWARE: Firmware = Firmware {
    words: pioc_proc::pioc_file_canonical!("firmware/rgb1w-100m.pio"),
    // The SFR path's delay loops are `nop` padding, so the image is tied to
    // the PIOC clock it was assembled for.
    sfr_bit_cycles: 117,
};

// `pioc` is the HAL's host driver; it implements `CommandPort`.
let bit_cycles = bit_cycles_for_hz(clocks().hclk.0).unwrap(); // 28/56/117 at 24/48/100 MHz
let mut ws = Ws2812::new(pioc, FIRMWARE, Config {
    output: OutputPin::Io0,
    storage: Storage::Auto,
    bit_cycles,
});
ws.write_rgb(&frame, ColorOrder::Grb).await?;
```

`Storage::Auto` uses the short SFR command when the frame fits 32 bytes, the
image's timing matches the configured clock, and no payload has been written
to program RAM yet. Writing one overwrites the tail of the image, so the driver
stays on the RAM path afterwards and an explicit `Storage::Sfr` is rejected.

Both examples in `ch32-hal` (`examples/ch32h417` and `examples/ch32x035`) use
this path end to end.

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
  --max-steps 100000 --max-cycles 10000000 \
  --trace trace.jsonl \
  --wavejson waveform.json \
  --vcd waveform.vcd
```

`translate` followed by `assemble --canonical` is how a vendor source becomes a
checked-in firmware image: the round trip has to reproduce the vendor `.BIN`
byte for byte, which is what makes a translated `.pio` reviewable rather than
an opaque blob.

`--max-steps` (default 100,000) caps executed instructions and `--max-cycles`
(default 10,000,000) caps simulated PIOC clocks.

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
PIOC cycles per WaveDrom slot. `--clock-hz` defaults to the standard 48 MHz
PIOC clock and controls both WaveDrom duration marks and VCD timestamps.

## Syntax and ISA

The parser accepts the vendor `INCLUDE`, `EQU`, `ORG`, `DW`, `DB` and `END`
directives and aliases used by the EVT sources. Canonical syntax deliberately
has a small core: familiar operations such as `mov`, `add`, `jmp`, `bset` and
`skip`, plus semantic register names such as `timer0.ctrl`, `pins.dir`,
`cmd.in`, `ptr0` and `d0`. `.alias` and `.const` are optional conveniences;
rare forms such as `mov.lit` are lossless-encoding escapes, not concepts
required for normal programs.

The translated EVT programs under [`examples/canonical`](examples/canonical) are
assembled in tests and must reproduce the vendor BIN files byte for byte.

See [docs/isa.md](docs/isa.md) for the complete instruction map and canonical
syntax, and [docs/registers.md](docs/registers.md) for every SFR and bitfield.
The high-level driver design and the Embassy comparison are in
[docs/programs-api-design.md](docs/programs-api-design.md).

## VM scope

Implemented today: deterministic CPU instructions, six-entry stack, flags,
indirect registers, host command handshake, GPIO edge waits, digital `0/1/Z/X`
resolution with pull-ups, Timer0 divider/reload/PWM output, PWM/Manchester codec
TX and normal-waveform RX, WAIT timeout reset, scheduled host/pin events, trace
JSONL, WaveJSON and VCD.

A host backend can also write program RAM (`Machine::write_program_ram`), which
is what a protocol driver does with a payload, and step firmware that waits on
a codec edge with `Machine::step_or_idle`.

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

Two layers of tests:

- The EVT golden files are checked whenever the sibling vendor tree is present,
  comparing every official ASM/BIN pair.
- The canonical tests need no vendor tree. They assemble the checked-in
  `rgb1w.pio` and run it in the VM, asserting the SFR waveform for one LED, the
  program-RAM payload and waveform for twelve, the sticky-RAM rule across
  alternating frame lengths, the IO1 pin claim and command collision, and an
  interrupt that was already pending before the wait. Status-code mapping and
  the SFR/RAM selection rules are covered against a recording mock port.

## Status

Working: the toolchain, the VM, and the CH32H417 / CH32X035 path through
`ch32-hal`.

Not implemented: the CH32V205 firmware ABI (`0x70` command, `0x600` payload,
low 4 KiB program SRAM), streaming protocols that need more than one pending
event, and a `Loaded` typestate. CH643's program-SRAM base is inferred from
CH32X0.

## License

Dual licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Each crate carries both files, so a crate can be packaged on its
own.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.

## Sources

- [WCH support thread containing the manual, assembler and examples](https://www.wch.cn/bbs/thread-111140-1.html)
- [Original `andelf/pioc` repository](https://github.com/andelf/pioc)

This project is an independent implementation; WCH product and instruction
names remain the property of their respective owner.
