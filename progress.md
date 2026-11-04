# PIOC research progress

Last updated: 2026-10-09  
Branch: `dev/pioc-research`

## Current status

The repository now contains a modular Rust PIOC toolchain:

- `pioc-core`: instruction, register, encoder and target-aware decoder model.
- `pioc-parser`: WCH-compatible and canonical assemblers, disassembler, and
  legacy-to-canonical translator.
- `pioc-proc`: compile-time `pioc_asm!`, `pioc_asm_canonical!`, and
  `pioc_file!` macros.
- `pioc-vm`: cycle-aware VM, host events, GPIO resolution, Timer0, codec,
  WaveJSON and VCD output.
- `pioc-programs`: backend-neutral, Embassy-style WS2812 and DS18x20 APIs.
- `pioc`: assembler, translator, disassembler, and VM CLI.

## ISA and assembler

- Documented the complete known PIOC instruction set, SFRs, and bitfields.
- Legacy assembly reproduces all five official EVT binaries byte for byte.
- Canonical disassembly reassembles losslessly across all 65,536 instruction
  words.
- Added target-aware decoding for generic RISC8B versus CH32X035 PIOC opcode
  interpretations.
- Added `.alias` and `.const` declarations.
- Added `pioc translate old.ASM -o new.pio`, preserving labels and comments
  while expanding includes.

The checked-in canonical EVT translations are:

- `examples/canonical/rgb1w.pio`
- `examples/canonical/i2c.pio`
- `examples/canonical/nec.pio`
- `examples/canonical/single-wire.pio`
- `examples/canonical/uart.pio`

Tests ensure these files remain synchronized with the translator and assemble
to the corresponding vendor BIN files.

## Canonical dialect policy

The canonical dialect is deliberately small. Its recommended vocabulary uses
familiar operations such as `mov`, `add`, `jmp`, `bset`, `bclr`, `skip`,
`call`, `ret`, and `wait`.

Examples:

```asm
.alias count = d0

mov a,#3
mov count,a
dec count
bset pins.dir0
bset pins.out0
skip clear,pins.in0
jmp nz,loop
```

Naming rules:

- `ptr0` and `ptr1` are pointer registers.
- `[ptr0]` and `[ptr1]` are indirect data views.
- `[ptr1+]` is emitted only for byte operations that post-increment.
- `[ptr1][n]` is used for non-incrementing bit access.
- Pin endpoints use `pins.in0`, `pins.in1`, `pins.out0`, and `pins.out1`.
- Old names remain accepted as migration aliases.

One- and two-operand forms are preferred (`dec ptr0`, `add a,count`,
`xor check,a`). Older explicit three-operand canonical forms remain accepted
for compatibility. Low-level forms such as `mov.lit a,#0` are reserved for
lossless encoding distinctions and are not part of normal source style.

## VM and waveform status

Implemented:

- deterministic instruction execution and instruction cycle counts;
- six-entry stack and arithmetic flags;
- indirect memory and operation-specific `ptr1` post-increment behavior;
- host command handshake and scheduled host/pin input events;
- GPIO `0/1/Z/X` resolution, pull-ups, ownership, reset, and clock gating;
- Timer0 divider, reload, and PWM output;
- PWM/Manchester codec TX and normal waveform RX model;
- WAIT behavior and timeout reset;
- JSONL trace, WaveJSON, and VCD generation.

The default WaveDrom resolution is two PIOC cycles per slot. `--clock-hz`
defaults to 48 MHz and waveform marks include cycle and physical duration.

Remaining calibration areas include exact codec receive phase, analog edge
behavior, same-cycle hardware priority, and the conflicting documentation for
`status` bits 4 and 5.

## High-level protocol API

`pioc-programs` currently provides:

- a `CommandPort` backend abstraction;
- `Ws2812` with RGB ordering, SFR/RAM selection, and IO1 command-collision
  protection;
- DS18x20 command mapping, parasite-power control, and signed temperature;
- an end-to-end VM test that drives one WS2812 LED through the official RGB1W
  firmware image.

UART, I2C, NEC, general OneWire, a real CH32X035 HAL backend, and typed loaded
program/ABI states remain future work.

## Program construction APIs

There is not yet a true Embassy-style `ProgramBuilder` or `Assembler` object.
Programs can currently be constructed in three ways:

1. `pioc_asm_canonical!` for checked, compile-time assembly with labels;
2. `assemble_canonical()` for runtime source assembly;
3. direct `Instruction` values followed by `encode()` for small typed tests.

The direct `Instruction` approach lacks labels, fixups, automatic address-zero
reservation, relocation, and whole-program validation. A future typed builder
should address these without expanding the assembly language itself.

## Verification baseline

The current baseline passes:

```console
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
```

It includes golden tests for all five official ASM/BIN pairs, canonical source
translation, all-word lossless disassembly, CLI translation, VM behavior, and
the high-level WS2812/DS18x20 APIs.

## Commits

- `2aee9b5` — modular PIOC toolchain and cycle-aware VM
- `a478ea3` — Embassy-style protocol APIs
- `312d24a` — ISA, simulation, and protocol API documentation
- `642599b` — default WaveDrom slots to two cycles
- `e669a18` — concise canonical PIOC dialect and EVT translations

## Suggested next steps

1. Design a minimal typed `ProgramBuilder` with labels and fixups while keeping
   the canonical assembly syntax simple.
2. Tighten canonical operand validation for fixed-operand encodings and expose
   explicit target profiles in all assembly entry points.
3. Add UART and I2C streaming protocol backends and intermediate-IRQ handling.
4. Implement the CH32X035 hardware HAL backend and validate VM timing against
   logic-analyzer captures.
