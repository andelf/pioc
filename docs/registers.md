# PIOC special-function registers

PIOC has 49 defined byte-wide SFRs: 17 fixed-function registers at `00h..0Ch`
and `1Ch..1F`, plus 32 shared data registers at `20h..3F`. `0Dh..1Bh` are
reserved. Canonical names describe machine roles; vendor names remain accepted
by the legacy assembler.

`R/W` below describes the PIOC core view. Host access differs for mixed-control
registers and is called out explicitly. Reset `X` means unspecified/retained.
The dotted names in the canonical columns are exactly what canonical assembly
accepts; a few short aliases such as `pins.output0` also resolve.

| Addr | Canonical | Vendor name | Reset | Core role |
|---:|---|---|---:|---|
| `00` | `[ptr0]` | `SFR_INDIR_PORT` | `XX` | data at address `ptr0` |
| `01` | `[ptr1]` | `SFR_INDIR_PORT2` | `XX` | data at `ptr1`; byte `MOV`/`MOVA` are written `[ptr1+]` |
| `02` | `pcl` | `SFR_PRG_COUNT` | `00` | low 8 PC bits; write changes control flow |
| `03` | `status` | `SFR_STATUS_REG` | mixed | ALU flags, stack/timeout and GP bits |
| `04` | `ptr0` | `SFR_INDIR_ADDR` | `XX` | indirect address 0 low byte |
| `05` | `timer0.count` | `SFR_TMR0_COUNT` | `00` | Timer0 current count |
| `06` | `timer0.ctrl` | `SFR_TIMER_CTRL` | `00` | Timer0 and pin-level controls |
| `07` | `timer0.reload` | `SFR_TMR0_INIT` | `00` | reload value / PWM threshold |
| `08` | `codec.period` | `SFR_BIT_CYCLE` | `00` | codec TX bit and period |
| `09` | `ptr1` | `SFR_INDIR_ADDR2` | `00` | indirect address 1 low byte |
| `0A` | `pins.dir` | `SFR_PORT_DIR` | `00` | pin mode, pull and direction |
| `0B` | `pins` | `SFR_PORT_IO` | `XX` | resolved input/XOR and output latch |
| `0C` | `codec.ctrl` | `SFR_BIT_CONFIG` | `10` | codec configuration and state |
| `0D..1B` | — | reserved | — | unspecified; preserve or write zero |
| `1C` | `sys.ctrl` | `SFR_SYS_CFG` | `00` | interrupt, handshake and host control |
| `1D` | `cmd.out` | `SFR_CTRL_RD` | `00` | PIOC writes, host reads |
| `1E` | `cmd.in` | `SFR_CTRL_WR` | `00` | host writes, PIOC reads |
| `1F` | `xchg` | `SFR_DATA_EXCH` | `00` | bidirectional exchange byte |
| `20..3F` | `d0..d31` | `SFR_DATA_REG0..31` | `00` | shared R/W software data |

## `status` (`03h`)

| Bit | Canonical role | Reset | Meaning |
|---:|---|---:|---|
| 7:6 | reserved | 0 | unspecified; the VM retains written bits |
| 5 | `status.stack_used`* | 0 | hardware says return stack is non-empty |
| 4 | `status.timeout_reset`* | 0 | enable Timer0 overflow reset while waiting |
| 3 | `status.gp_y` | 0 (power-on) | software general-purpose bit Y |
| 2 | `status.zero` | 0 | ALU zero flag |
| 1 | `status.gp_x` | 0 (power-on) | software general-purpose bit X |
| 0 | `status.carry` | 0 | carry/borrow/shift flag |

Canonical bit operands use exactly these fully qualified names, for example
`bset status.zero`.

`*` The PDF and official `PIOC_INC.ASM` exchange bits 5 and 4. The table follows
the PDF. Existing examples do not resolve the conflict; hardware calibration is
required before relying on timeout reset.

## `timer0.ctrl` (`06h`)

| Bit | Canonical role | Meaning |
|---:|---|---|
| 7 | `timer0.ctrl.level1_enable` | IO1 level change can wake/request interrupt |
| 6 | `timer0.ctrl.level0_enable` | IO0 level change can wake/request interrupt |
| 5 | `timer0.ctrl.enable` | run Timer0 |
| 4 | `timer0.ctrl.output_enable` | route timer/PWM output to IO0 |
| 3 | `timer0.ctrl.pwm_mode` | 0=reload timer; 1=free-running PWM |
| 2:0 | `timer0.ctrl.divider2`..`divider0` | `/1024,/256,/64,/16,/8,/4,/2,/1` |

Timer mode increments from `timer0.reload` through `FF`, then reloads. Its
period is `divider × (256-reload)` core clocks. PWM mode is always 256 timer
ticks and is high while `count > reload`, giving documented duty
`(255-reload)/256`.

Host view warning: header/prose map host GP Y/X onto this register's bits 5/4,
while the PDF access summary appears to put those swapped views on address 05.

## `codec.period` (`08h`)

| Bit | Canonical role | Meaning |
|---:|---|---|
| 7 | `codec.period.tx_bit` | next raw TX bit; double-buffered at bit boundary |
| 6:0 | `codec.period.period6`..`period0` | bit-cycle count (vendor descriptions differ by one) |

Writing this register clears IO0 edge records. A zero period stops the codec and
sets `codec.done`; the manual says very small values with the high five
period bits clear disable encoding. Exact off-by-one timing remains a hardware
calibration point; official examples use values such as 28 and 56 clocks.

## `pins.dir` (`0Ah`)

| Bit | Canonical role | Meaning |
|---:|---|---|
| 7 | `pins.dir.mode3` | implementation/host-defined pad mode |
| 6 | `pins.dir.mode2` | implementation/host-defined pad mode |
| 5 | `pins.dir.mode1` | implementation/host-defined pad mode |
| 4 | `pins.dir.mode0` | implementation/host-defined pad mode |
| 3 | `pins.dir.pull1` | enable IO1 internal pull-up |
| 2 | `pins.dir.pull0` | enable IO0 internal pull-up |
| 1 | `pins.dir1` | IO1: 0=input/high-Z, 1=output |
| 0 | `pins.dir0` | IO0: 0=input/high-Z, 1=output |

## `pins` (`0Bh`)

| Bit | Canonical role | Access | Meaning |
|---:|---|---|---|
| 7 | `pins.input_xor` | R | `input1 XOR input0` |
| 6 | `codec.rx_bit` | R | bit decoded from IO0 |
| 5 | `pins.in1` | R | resolved IO1 level |
| 4 | `pins.in0` | R | resolved IO0 level |
| 3 | `pins.changed1` | R | `in1 XOR out1` |
| 2 | `pins.changed0` | R | `in0 XOR out0` |
| 1 | `pins.out1` | R/W | IO1 output latch |
| 0 | `pins.out0` | R/W | IO0 output latch |

Timer0 output and codec TX can override the IO0 output latch. The `BP*`/`BG*`
instructions can transfer pin/codec endpoints without a general register move.
The VM resolves PIOC drive, external drive and internal pull-up into a physical
`0/1/Z/X` level; changing direction to input therefore models open-drain release.

## `codec.ctrl` (`0Ch`)

| Bit | Canonical role | Access | Meaning |
|---:|---|---|---|
| 7 | `codec.ctrl.tx_enable` | W | 0=receive/decode; 1=encode/transmit on IO0 |
| 6 | `codec.ctrl.manchester` | W | 0=25/75% PWM; 1=Manchester-style coding |
| 5 | `codec.ctrl.edge_sample` | W | select present midpoint or previous-edge sample |
| 4 | `codec.done` | R | final quarter of bit cycle / codec completion |
| 3 | `codec.ctrl.count6` | R | internal cycle counter bit 6 |
| 2 | `codec.ctrl.count5` | R | internal cycle counter bit 5 |
| 1 | `codec.ctrl.count4` | R | internal cycle counter bit 4 |
| 0 | `codec.ctrl.count3` | R | internal cycle counter bit 3 |

The VM implements PWM and Manchester TX plus midpoint-based normal-waveform RX.
The manual does not fully specify receive phase and `edge_sample` boundaries, so
those exact boundaries remain subject to logic-analyzer calibration.

## `sys.ctrl` (`1Ch`)

| Bit | Canonical role | Controller | Meaning |
|---:|---|---|---|
| 7 | `sys.irq_request` | PIOC | request host interrupt; host false-write to `cmd.out` clears |
| 6 | `sys.ctrl.core_to_host_pending` | handshake | set by PIOC `cmd.out` write; host read clears |
| 5 | `sys.ctrl.host_to_core_pending` | handshake | set by host `cmd.in` write; PIOC read clears |
| 4 | `sys.ctrl.host_config` | host | application-defined configuration |
| 3 | `sys.ctrl.host_claim_io1` | host | 1 gives IO1 to PIOC, 0 to host |
| 2 | `sys.ctrl.host_claim_io0` | host | 1 gives IO0 to PIOC, 0 to host |
| 1 | `sys.ctrl.host_reset` | host | force PIOC reset |
| 0 | `sys.ctrl.host_clock_run` | host | run core; 0 gates clock and exposes SRAM |

The PDF description for bit 2 says IO1 in one place; its macro name, host
header and examples identify it as IO0.

## Command and shared registers

- Writing `cmd.out` sets `sys.ctrl.core_to_host_pending`; host reading it clears
  the pending bit. Legacy name `SFR_CTRL_RD` is from the host's read direction.
- A host false write to `cmd.out` (any value, data ignored) clears
  `sys.irq_request`. Official host code uses this after handling an interrupt;
  the PIOC itself also clears the request once the pending byte has been read.
- Host writing `cmd.in` sets `sys.ctrl.host_to_core_pending`; PIOC reading it
  clears the pending bit. Legacy name `SFR_CTRL_WR` is from the host direction.
- `xchg` is ordinary shared data with special single-cycle bit-transfer paths.
- `d0..d31` have no hardware-prescribed meaning and are shared with the host.

Per the PIOC manual, byte `MOV`/`MOVA` accesses increment `ptr1` and therefore
use `[ptr1+]`; bit tests, bit set/clear and arithmetic accesses use `[ptr1]`
without increment. A bit operand is written `[ptr1][bit]`.
`MOVA2P` is the separate immediate-write-and-increment instruction.
