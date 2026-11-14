# Embassy-style `pioc-programs` high-level protocol API: comparison, design, and feasibility

> Status: the design has a compilable prototype. The prototype implements
> `CommandPort`, WS2812/RGB, and the DS18x20 operations provided by the official
> RGB1W firmware, and performs one end-to-end VM check of a single WS2812 LED
> against the official `RGB1W.BIN`. UART, I²C, generic OneWire, and a real
> CH32X035 hardware backend belong to later phases.

## 1. Conclusion

The most valuable thing to borrow from Embassy RP PIO is not RP2040's FIFO,
side-set, or multiple state machines themselves, but the following software
structure:

1. separate the "program image" from the protocol driver that uses it;
2. the driver exclusively owns the state machine, pins, DMA, and other
   resources, and Rust ownership prevents concurrent misuse;
3. provide both raw low-level entry points and protocol APIs such as
   `write_slice`, `transfer`, and `reset`;
4. blocking, asynchronous FIFO, DMA, and IRQ share one set of protocol
   semantics;
5. the program ABI, pins, clock, and buffer layout are encapsulated by the
   driver instead of being exposed to the application.

WCH PIOC can adopt the same layering, but it must not copy RP PIO's concrete
execution model. PIOC is an 8-bit programmable core that uses `cmd.in`/`cmd.out`,
32 bytes of shared SFRs, and shared program SRAM; it has no four independent
state machines and no general TX/RX FIFO. The main abstraction should therefore
be `CommandPort`/`EventPort`, not an imitation of an RP-style FIFO.

The current validation shows that the following design is feasible:

- the protocol crate can stay `no_std`, allocation-free, and
  executor-agnostic;
- the same high-level `Ws2812` driver can run on a hardware backend or a VM
  backend;
- the API can automatically choose between the short SFR and long RAM firmware
  paths;
- firmware command collisions, capacity limits, and status codes can be
  reported explicitly as typed errors;
- the async boundary sits at "wait for the PIOC IRQ", matching the official
  interrupt-driven UART/I²C examples.

## 2. Research basis

### 2.1 Embassy RP PIO

| Topic | Reference implementation |
|---|---|
| Inline assembly and multi-state-machine async tasks | `examples/rp/src/bin/pio_async.rs` |
| TX/RX DMA | `examples/rp/src/bin/pio_dma.rs` |
| Blocking and async SPI | `pio_spi.rs`, `pio_spi_async.rs` |
| WS2812 program object, DMA, and color order | `embassy-rp/src/pio_programs/ws2812.rs` |
| OneWire reset/read/write/search/strong pull-up | `embassy-rp/src/pio_programs/onewire.rs` |
| UART TX/RX split and `embedded-io-async` | `embassy-rp/src/pio_programs/uart.rs` |
| IRQ future/flag | `tests/rp/src/bin/pio_irq.rs` |
| Automatic program relocation and space management | `tests/rp/src/bin/pio_multi_load.rs` |

Embassy shows three ways of organizing programs:

- `pio_asm!` inline assembly;
- the `pio::Assembler` typed builder;
- `PioXxxProgram + PioXxx` reusable protocol drivers.

The current Embassy tree contains no RP example that uses external `.pio` files
or `pio_file!`.

### 2.2 WCH PIOC

Main sources:

- `PIOC-EN.pdf`, `CHRISC8B-EN.pdf`;
- the official `RGB1W.ASM`/`BIN`;
- the `RGB1W.c`/`.h` host code;
- the official UART and I²C host interrupt/double-buffer code;
- this repository's assembler, VM, SFR documentation, and end-to-end traces.

The official RGB1W host code already demonstrates two data paths:

| Mode | Capacity | Load condition | Start method |
|---|---:|---|---|
| SFR short | 1..32 bytes | PIOC running; write `d0..d31` | `cmd.in = length` or `length\|0x40` |
| program-RAM long | 1..3072 bytes | PIOC reset/clock-stopped; write SRAM `0x400..0xFFF` | `cmd.in = 0x80\|bit_cycles` |

The official UART and I²C examples further show that:

- an IRQ does not only mean final completion; it can also mean "host, please
  refill the next buffer";
- UART alternates between two 8-byte SFR buffers;
- I²C uses multiple 8-byte buffers for controller/target directions;
- the backend must support waiting for IRQs repeatedly and must not hard-code
  every IRQ as final completion.

## 3. Key differences between RP PIO and WCH PIOC

| Capability | RP PIO / Embassy | WCH PIOC | API impact |
|---|---|---|---|
| Execution units | multiple state machines per block | one RISC8B/PIOC core | the driver owns the whole PIOC by default |
| Program space | 32 words, shared and relocatable | 2048-word full image | no generic fragment relocation needed |
| Data channels | TX/RX FIFO, DMA pacing | `cmd.*`, `xchg`, `d0..d31`, shared SRAM | use a command/event backend |
| Pin mapping | per-SM IN/OUT/SET/side-set configuration | IO0/IO1 plus chip pin remap | pins are part of the firmware ABI |
| Clock | per-SM divider | PIOC follows the system clock; firmware delays depend on frequency | the program profile must record the clock |
| IRQ | multiple PIO IRQ flags | PIOC requests a host IRQ | the backend normalizes them into async events |
| Concurrency | multiple SMs run in parallel | one core runs one firmware image at a time | no fake multi-driver concurrency |
| Data flow | FIFO streams are natural | firmware-defined double buffering/handshake | streaming must model the firmware ABI |

## 4. Goals and non-goals

### 4.1 Goals

- `no_std`, with no allocator dependency by default;
- executor-agnostic, so it can serve Embassy, polling HALs, and the VM;
- hardware and VM reuse the same protocol drivers;
- Rust ownership serializes access to the PIOC;
- hide addresses, command numbers, buffer layout, and interrupt-acknowledgment
  details;
- support short commands, long payloads, and IRQ streaming;
- state the firmware ABI version, clock profile, pin, and RAM requirements
  explicitly;
- preserve backend errors and PIOC firmware status in errors;
- tests can check the command sequence and can also run real official BIN
  images.

### 4.2 Non-goals

- do not pretend that WCH PIOC is RP PIO;
- do not promise that one firmware image runs multiple unrelated protocols
  concurrently;
- do not bind a concrete NVIC/executor inside the protocol crate;
- do not put external-device models into the hardware HAL;
- do not package WCH binaries whose license has not been confirmed into a
  redistributable crate;
- do not describe VM digital results as silicon-level electrical validation.

## 5. Proposed crate layering

```text
pioc-core
  Instruction / Register base types (the program-image type is Program in
  pioc-parser)

pioc-parser + pioc-proc
  assembly, disassembly, compile-time program generation

pioc-hal-ch32x035                 (proposed)
  registers, SRAM loading, reset/run, IRQ future, pin remap

pioc-programs                    (prototype added)
  backend-neutral protocol API
  ws2812 / ds18x20 / uart / i2c / nec / onewire-v2

pioc-vm
  instructions, SFR, GPIO, Timer, codec, trace

pioc-vm-adapter                  (can be folded into a pioc-programs feature)
  CommandPort/EventPort for Machine
```

The application layer depends only on `pioc-programs` and one backend; it does
not need to know what `d7` or `0x33` means.

## 6. Implemented minimal backend contract

The prototype is in `pioc-programs/src/lib.rs`:

```rust
pub trait CommandPort {
    type Error;

    fn reset_halted(&mut self, pins: PinMask) -> Result<(), Self::Error>;
    fn run(&mut self, pins: PinMask) -> Result<(), Self::Error>;
    fn set_host_config(&mut self, enabled: bool) -> Result<(), Self::Error>;

    fn write_data(&mut self, offset: u8, data: &[u8])
        -> Result<(), Self::Error>;
    fn read_data(&mut self, offset: u8, data: &mut [u8])
        -> Result<(), Self::Error>;

    fn write_program_ram(&mut self, offset: u16, data: &[u8])
        -> Result<(), Self::Error>;
    fn program_ram_capacity(&self) -> usize;

    fn start_command(&mut self, command: u8) -> Result<(), Self::Error>;

    fn wait_for_interrupt(&mut self)
        -> impl Future<Output = Result<u8, Self::Error>> + '_;
}
```

### 6.1 Why not a FIFO trait

For command-style firmware, `wait_for_interrupt()` means completion. For
UART/I²C streaming firmware it can be called repeatedly, and after each return
the protocol driver checks data-area flags, refills a buffer, or takes RX data.
This avoids incorrectly abstracting `cmd.out` as an RP TX/RX FIFO.

### 6.2 Why return `impl Future`

- no `async-trait` or heap allocation is needed;
- `no_std` is preserved;
- the hardware backend returns an IRQ future;
- the VM backend can return a "run until IRQ" future;
- dispatch is static, matching embedded resource-ownership models.

The trait is not object-safe; that is intentional. PIOC drivers usually know
the backend type at compile time.

### 6.3 Lower-level interfaces to split out later

To express all firmware correctly, the second phase should split the contract
into:

```rust
trait PiocControl {
    fn reset_halted(...);
    fn load_image(...);
    fn run(...);
    fn abort(...);
}

trait PiocRegisters {
    fn read_data(...);
    fn write_data(...);
    fn read_xchg(...);
    fn write_xchg(...);
    fn write_program_ram(...);
}

trait PiocEvents {
    fn start_command(...);
    fn wait_irq(...) -> impl Future<...>;
    fn read_command_out(...);
    fn acknowledge_irq(...);
}
```

The current `CommandPort` is a proven MVP. After the split it can fully support
the intermediate IRQs of UART/I²C and leave chip details such as "reading
`cmd.out` clears the IRQ automatically" and "a spurious `cmd.out` write clears
the IRQ" to the backend.

## 7. Program and ABI types

Embassy uses `PioWs2812Program` for a program already loaded into instruction
RAM. The PIOC counterpart should not be a bare `&[u8]`. Proposed:

```rust
pub trait ProgramSpec {
    const ABI: ProgramAbi;
    fn image(&self) -> &[u8];
}

pub struct ProgramAbi {
    pub name: &'static str,
    pub version: AbiVersion,
    pub required_clock: ClockRequirement,
    pub pins: PinMask,
    pub program_words: u16,
    pub payload_ram: Option<Range<u16>>,
    pub command_set: &'static [CommandDescriptor],
}

pub struct Loaded<B, P> {
    backend: B,
    program: P,
}
```

Expected usage:

```rust
let pioc = PiocManager::new(peripheral, irq);
let loaded = pioc.load(Rgb1wProgram::new(RGB1W_BIN)).await?;
let mut leds = Ws2812::new(loaded, config);
```

The `Loaded<B, Rgb1wProgram>` type prevents binding a UART driver to an RGB1W
image.

To validate the protocol layer first, the prototype constructor currently
assumes the caller has already loaded the correct firmware. A production
hardware API should introduce the typestate above.

## 8. Error model

The prototype already implements:

```rust
pub enum Error<E> {
    Backend(E),
    EmptyPayload,
    PayloadTooLong { actual: usize, maximum: usize },
    SfrCommandCollision { length: usize, command: u8 },
    ProgramRamUnavailable,
    InvalidBitCycles(u8),
    Firmware(FirmwareError),
}
```

The official RGB1W status values map to:

| status | Typed error |
|---:|---|
| `0` | success |
| `1` | `UnknownCommand` |
| `2` | `InvalidParameter` |
| `4` | `OutputHigh` |
| `6` | `InputHigh` |
| other | `Other(code)` |

A production version should also add:

- `Busy`;
- `Timeout`;
- `WrongProgramAbi`;
- `UnsupportedClock`;
- `PinUnavailable`;
- `CancelledNeedsRecovery`;
- overrun/underrun and parity/framing/NACK/arbitration errors for streaming.

## 9. WS2812 API

### 9.1 Current API

```rust
let config = ws2812::Config {
    output: OutputPin::Io0,
    storage: Storage::Auto,
    bit_cycles: 56,
};

let mut leds = Ws2812::new(port, config);

leds.write_rgb(
    &[Rgb8::new(255, 0, 0)],
    ColorOrder::Grb,
).await?;
```

It can also send a pre-encoded byte stream directly:

```rust
leds.write_raw(&[0x00, 0xff, 0x00]).await?;
```

Six RGB orders are supported: `RGB/RBG/GRB/GBR/BRG/BGR`. The firmware itself
sends arbitrary bytes, so RGBW or custom LEDs can start with `write_raw`, and a
production version can add `Rgbw8` and a color-order trait later.

### 9.2 Automatic storage-path selection

`Storage::Auto` uses these rules:

1. empty payload: error;
2. 1..32 bytes and no short-command collision: use SFR;
3. any other length: use program RAM;
4. RAM backend unavailable: `ProgramRamUnavailable`;
5. above the backend capacity or the official 3072-byte limit:
   `PayloadTooLong`.

The implementation needs no heap: `write_rgb(&[Rgb8])` packs each pixel into a
local `[u8; 3]` and then writes the SFRs or program RAM while the clock is
stopped.

### 9.3 IO1 command collision

The official short mode uses:

```text
IO0 command = length
IO1 command = 0x40 | length
```

while the same firmware first interprets:

```text
0x41 = DS1W convert
0x42 = DS1W read
0x43 = DS1W convert + read
```

Sending 1, 2, or 3 bytes on IO1 therefore collides. One RGB LED is exactly
3 bytes, and `0x43` would wrongly enter the temperature-conversion path.
Prototype behavior:

- `Storage::Auto`: switch to RAM mode automatically;
- `Storage::Sfr`: return `SfrCommandCollision`;
- never silently send a wrong command.

This is a direct example of the high-level API being safer than the official C
wrapper.

### 9.4 Clock

Short-mode software delays depend on a frequency patch in the firmware image;
in long mode the low 7 bits of the command carry `bit_cycles` directly.
Official values:

| PIOC/system clock | bit cycles |
|---:|---:|
| 48 MHz | 56 |
| 24 MHz | 28 |

A production `ProgramSpec` should apply the short-mode patch at load time and
compute/validate long-mode bit cycles from the actual clock. The current
prototype requires the user to configure `bit_cycles` correctly and limits it
to the firmware-accepted `8..=124`.

## 10. OneWire and DS18x20

### 10.1 Embassy `PioOneWire` cannot be copied directly

The Embassy RP implementation provides:

```text
reset
write_bytes
read_bytes
write_bytes_pullup
ROM search
```

The official WCH RGB1W firmware only exports:

```text
0x41 convert
0x42 read first two scratchpad bytes
0x43 convert, wait 750 ms, read
```

The internal firmware does have reset/read/write subroutines, but they are not
exposed through the host ABI. `pioc-programs` is therefore correctly named
`Ds18x20` rather than claiming to implement a generic `OneWireBus`.

### 10.2 Current DS18x20 API

```rust
let mut sensor = Ds18x20::new(port);

sensor.start_conversion(PowerMode::Parasite).await?;
// The host waits for the sensor conversion time.
let temperature = sensor.read_temperature().await?;

let milli_c = temperature.milli_celsius();
```

The firmware's internal blocking 750 ms command is also supported:

```rust
let temperature = sensor
    .convert_and_read(PowerMode::External)
    .await?;
```

`PowerMode::Parasite` maps to `sys.ctrl.host_config`; after sending `44h` the
firmware actively drives IO0 high to provide the strong pull-up.

### 10.3 Proposed generic OneWire V2 firmware

Reaching Embassy-level capability requires a new PIOC firmware ABI:

```text
01h RESET                  -> presence/status
02h WRITE                  d0=len, d1.. payload
03h READ                   d0=len, result in data/RAM
04h TRIPLET                search direction -> id/complement/result
05h STRONG_PULLUP_BEGIN
06h STRONG_PULLUP_END
07h TOUCH_BITS             simultaneous write/read slots
```

The corresponding API:

```rust
trait OneWireBus {
    async fn reset(&mut self) -> Result<bool, Error>;
    async fn write(&mut self, bytes: &[u8]) -> Result<(), Error>;
    async fn read(&mut self, bytes: &mut [u8]) -> Result<(), Error>;
    async fn triplet(&mut self, direction: bool) -> Result<Triplet, Error>;
}
```

ROM search should live in the Rust protocol layer, with PIOC executing only
precisely timed triplets. This is easier to test than embedding the full search
state machine in PIOC, and it allows multiple sensors.

## 11. UART and I²C streaming API

### 11.1 UART

The official UART alternates between two 8-byte TX buffers; an IRQ may mean:

- an RX byte is ready;
- TX buffer A can be refilled;
- TX buffer B can be refilled;
- transmission is complete or failed.

Proposed API:

```rust
pub struct PiocUart<B> { /* owns backend and buffer state */ }

impl<B> PiocUart<B> {
    pub fn split(self) -> (UartTx<B>, UartRx<B>);
    pub fn set_config(&mut self, config: UartConfig) -> Result<(), Error>;
}

impl embedded_io_async::Write for UartTx<_> { ... }
impl embedded_io_async::Read for UartRx<_> { ... }
```

Note that a single PIOC backend cannot be divided directly between two owners
the way RP's two state machines can. The implementation should use shared
internal state with a single IRQ dispatcher, or maintain TX/RX ring buffers
inside one `run()` task and expose pipe/channel handles to the application.

### 11.2 I²C

Implement the controller and target separately:

```rust
PiocI2cController<B>
    write(address, bytes)
    read(address, bytes)
    write_read(address, tx, rx)

PiocI2cTarget<B>
    run(handler).await
```

The controller can implement `embedded-hal-async::i2c::I2c`. The target needs
continuous IRQ service and cannot be modeled as a one-shot `command().await`.

## 12. Ownership, concurrency, and cancellation

### 12.1 Exclusive ownership

The protocol driver owns the backend:

```rust
pub struct Ws2812<B> {
    port: B,
    config: Config,
}
```

All methods take `&mut self`, so the compiler prevents two commands from
modifying the same PIOC at the same time. `release()` returns the backend,
allowing the application to switch firmware/protocol explicitly.

### 12.2 Cancellation safety

If the future of `write_rgb().await` is dropped after `cmd.in` has been written,
PIOC keeps running. This is similar to DMA/FIFO drivers, and the API must not
claim cancellation safety.

A production version should provide:

```rust
driver.is_busy();
driver.wait_idle().await;
driver.abort().await;       // reset/halt and restore known state
driver.recover().await;     // reload/restart firmware when required
```

The documentation must state that dropping an operation future does not cancel
the hardware transaction.

### 12.3 Timeouts

The protocol crate should not bind `embassy_time::Duration`. Two options:

- the application wraps the future with the executor's `with_timeout`;
- the backend provides an optional deadline/timeout adapter.

After a timeout, call `abort/recover` so that the next operation does not treat
a stale IRQ as a new completion.

## 13. Hardware backend sketch

Responsibilities of the CH32X035 backend:

```rust
struct Ch32x035Pioc<'d> {
    regs: PeripheralRef<'d, PIOC>,
    irq_state: &'d IrqState,
    payload_ram_capacity: usize,
}
```

Implementation order:

1. configure the AFIO/GPIO remap;
2. `reset_halted` writes `sys.ctrl` reset/ownership;
3. while the clock is stopped, copy the program image or long payload into
   shared SRAM;
4. `run` releases reset and enables the clock;
5. `write_data`/`read_data` use volatile SFR accesses;
6. `start_command` writes `cmd.in` last, forming the release boundary;
7. the IRQ handler only clears the NVIC source, records the event, and wakes;
8. after the future is polled, read and acknowledge the PIOC IRQ and `cmd.out`.

Critical sections must guarantee that:

- `cmd.in` is written only after the entire command payload;
- the IRQ handler and the task never refill the same buffer concurrently;
- PIOC is really clock-stopped while RAM mode writes SRAM;
- same-cycle host/PIOC SFR writes follow the hardware "host wins" rule.

## 14. VM backend

The VM backend should implement the same trait:

| `CommandPort` method | VM behavior |
|---|---|
| `reset_halted` | host write `sys.ctrl` reset/claims |
| `run` | host write clock-run/claims |
| `write_data` | host write `d0..d31` |
| `write_program_ram` | write the shared program SRAM window exposed by the VM |
| `start_command` | host write `cmd.in` |
| `wait_for_interrupt` | run the VM until `interrupt_request` |

The current end-to-end test implements a minimal VM adapter inside the test
file, covering the SFR WS2812 path. The VM does not yet expose an interface for
"the host writes program SRAM while the clock is stopped", so the long/RAM path
is currently validated by mocks only. The next step should add a host
program-memory API to `Machine` constrained by clock-gate/reset, instead of
having the adapter modify the private `program` array.

## 15. Feasibility validation

### 15.1 Compile properties

- `pioc-programs` uses `#![no_std]`;
- production dependencies are empty;
- no allocator is required;
- async trait boundaries need no boxing;
- the workspace rule forbidding unsafe code is still in effect.

### 15.2 Mock behavior tests

Verified:

1. a pure red LED is encoded as `00 FF 00`;
2. a 3-byte IO0 payload automatically selects the SFR command `03h`;
3. a 3-byte IO1 payload automatically switches to RAM mode;
4. forcing IO1 SFR mode returns `SfrCommandCollision(0x43)`;
5. RAM mode writes payload, length, pin, and `0x80|bit_cycles`;
6. DS18x20 commands `41h`/`42h` and the parasite flag map correctly;
7. signed raw temperature values convert to milli-Celsius.

### 15.3 Official BIN end-to-end test

The test loads the official `RGB1W.BIN` into `pioc-vm::Machine` and then uses
only the high-level API (the test depends on the external WCH EVT directory and
is skipped when it is absent):

```rust
driver
    .write_rgb(&[Rgb8::new(255, 0, 0)], ColorOrder::Grb)
    .await?;
```

The IO0 high pulse widths in the VM trace decode back to:

```text
00000000 11111111 00000000
   G        R        B
```

The test never writes `d0`, `cmd.in`, or the PC directly, proving that the loop
from the high-level API to the official firmware ABI is closed.

### 15.4 Not yet verified

- CH32X035 real-hardware IRQ future;
- VM/real-hardware end-to-end path for RAM long mode;
- IO1 long-mode output on real hardware;
- DS18x20 presence, read, and parasite strong pull-up against an active device
  model;
- UART/I²C long-payload streaming;
- the 24 MHz firmware patch;
- cancellation, timeout, and interrupt races.

## 16. Performance and capacity

### 16.1 WS2812

| Path | Max data | RGB LEDs | Host involvement |
|---|---:|---:|---|
| SFR | 32 bytes | 10 LEDs + 2 bytes | write the shared SFRs once |
| RAM | 3072 bytes | 1024 LEDs | write SRAM once or in chunks while the clock is stopped |

RAM mode should not run/halt after every pixel; the prototype stops the clock,
writes all pixels contiguously, and starts once.

### 16.2 UART/I²C

Official double buffering means heap-free streaming is possible, but the
worst-case IRQ service latency must be less than the time one buffer drains at
line rate. API documentation should provide:

```text
service_budget = buffer_bytes * bits_per_byte / baud
```

At high baud rates one can:

- raise the IRQ priority;
- use a larger shared-SRAM protocol;
- add DMA inside the hardware backend if the chip offers a suitable DMA route;
- never promise RP-style DMA FIFOs from the generic protocol layer.

## 17. Proposed implementation phases

### Phase 1: prototype (done)

- `CommandPort`;
- typed error/status;
- `Ws2812` SFR/RAM selection;
- `Ds18x20` legacy API;
- mock tests;
- VM end-to-end test against the official RGB1W BIN.

### Phase 2: hardware and VM backends

- `ProgramSpec`, `ProgramAbi`, `Loaded<B,P>`;
- CH32X035 volatile register/SRAM backend;
- Embassy-compatible IRQ future;
- VM host program-RAM API;
- timeout, abort, recover;
- real-hardware logic-analyzer calibration at 24/48 MHz.

### Phase 3: streaming

- split `PiocControl`/`PiocRegisters`/`PiocEvents`;
- UART TX/RX double-buffered state machine;
- `embedded-io-async` traits;
- I²C controller `embedded-hal-async` trait;
- target/IRQ service task;
- overrun, framing, NACK, and similar errors.

### Phase 4: generic OneWire and protocol ecosystem

- new OneWire V2 firmware ABI;
- reset/read/write/triplet/strong-pullup;
- ROM search and DS18B20 scratchpad/CRC;
- NEC, single-wire debug, I²C target;
- shared protocol test vectors between the hardware backend and the VM peer
  model.

## 18. Final recommendation

`pioc-programs` should adopt Embassy's "Program + owned Driver + async
operation" idea while centering on WCH's command/SFR/IRQ ABI. The most
reasonable public API in the near term is:

```text
ProgramSpec -> Loaded<B, Program>
             -> Ws2812 / Ds18x20 / Uart / I2c
             -> async protocol methods
```

Do not expose an API that requires users to hand-fill `d0`, `d7`, or
`cmd.in=0x33`, and do not hide different firmware buffer ABIs behind an
over-generalized FIFO. The protocol driver should be the sole interpreter of the
firmware ABI; the backend is only responsible for reliably loading, reading and
writing, waiting for IRQs, and recovering the hardware.

The current prototype has shown that this direction is feasible on the Rust type
system, `no_std`, the official RGB1W firmware, and the VM. The highest-value
next step is to implement the formal `ProgramSpec`/`Loaded` typestate and the
two backends (CH32X035/VM), then validate the streaming event model with UART
double buffering.
