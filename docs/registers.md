# PIOC special-function registers

PIOC has 49 defined byte-wide SFRs: 17 fixed-function registers at `00h..0Ch`
and `1Ch..1F`, plus 32 shared data registers at `20h..3F`. `0Dh..1Bh` are
reserved. Canonical names describe machine roles; vendor names remain accepted
by the legacy assembler.

`R/W` below describes the PIOC core view. Host access differs for mixed-control
registers and is called out explicitly. Reset `X` means unspecified/retained.

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
| `0D..1B` | — | reserved | — | read zero; preserve or write zero |
| `1C` | `sys.ctrl` | `SFR_SYS_CFG` | `00` | interrupt, handshake and host control |
| `1D` | `cmd.out` | `SFR_CTRL_RD` | `00` | PIOC writes, host reads |
| `1E` | `cmd.in` | `SFR_CTRL_WR` | `00` | host writes, PIOC reads |
| `1F` | `xchg` | `SFR_DATA_EXCH` | `00` | bidirectional exchange byte |
| `20..3F` | `d0..d31` | `SFR_DATA_REG0..31` | `00` | shared R/W software data |

## `status` (`03h`)

| Bit | Canonical role | Reset | Meaning |
|---:|---|---:|---|
| 7:6 | reserved | 0 | read zero |
| 5 | `stack_used`* | 0 | hardware says return stack is non-empty |
| 4 | `wait_timeout_reset`* | 0 | enable Timer0 overflow reset while waiting |
| 3 | `gp_y` | 0 (power-on) | software general-purpose bit Y |
| 2 | `zero` | 0 | ALU zero flag |
| 1 | `gp_x` | 0 (power-on) | software general-purpose bit X |
| 0 | `carry` | 0 | carry/borrow/shift flag |

Canonical bit operands use fully qualified names, for example `status.zero`.

`*` The PDF and official `PIOC_INC.ASM` exchange bits 5 and 4. The table follows
the PDF. Existing examples do not resolve the conflict; hardware calibration is
required before relying on timeout reset.

## `timer0.ctrl` (`06h`)

| Bit | Canonical role | Meaning |
|---:|---|---|
| 7 | `level1.enable` | IO1 level change can wake/request interrupt |
| 6 | `level0.enable` | IO0 level change can wake/request interrupt |
| 5 | `timer0.enable` | run Timer0 |
| 4 | `timer0.output_enable` | route timer/PWM output to IO0 |
| 3 | `timer0.pwm_mode` | 0=reload timer; 1=free-running PWM |
| 2:0 | `timer0.divider` | `/1024,/256,/64,/16,/8,/4,/2,/1` |

Timer mode increments from `timer0.reload` through `FF`, then reloads. Its
period is `divider × (256-reload)` core clocks. PWM mode is always 256 timer
ticks and is high while `count > reload`, giving documented duty
`(255-reload)/256`.

Host view warning: header/prose map host GP Y/X onto this register's bits 5/4,
while the PDF access summary appears to put those swapped views on address 05.

## `codec.period` (`08h`)

| Bit | Canonical role | Meaning |
|---:|---|---|
| 7 | `codec.tx_bit` | next raw TX bit; double-buffered at bit boundary |
| 6:0 | `codec.period` | bit-cycle count (vendor descriptions differ by one) |

Writing this register clears IO0 edge records. A zero period stops the codec and
sets `codec.done`; the manual says very small values with the high five
period bits clear disable encoding. Exact off-by-one timing remains a hardware
calibration point; official examples use values such as 28 and 56 clocks.

## `pins.dir` (`0Ah`)

| Bit | Canonical role | Meaning |
|---:|---|---|
| 7 | `mode3` | implementation/host-defined pad mode |
| 6 | `mode2` | implementation/host-defined pad mode |
| 5 | `mode1` | implementation/host-defined pad mode |
| 4 | `mode0` | implementation/host-defined pad mode |
| 3 | `pull1` | enable IO1 internal pull-up |
| 2 | `pull0` | enable IO0 internal pull-up |
| 1 | `dir1` | IO1: 0=input/high-Z, 1=output |
| 0 | `dir0` | IO0: 0=input/high-Z, 1=output |

## `pins` (`0Bh`)

| Bit | Canonical role | Access | Meaning |
|---:|---|---|---|
| 7 | `input_xor` | R | `input1 XOR input0` |
| 6 | `codec.rx_bit` | R | bit decoded from IO0 |
| 5 | `in1` | R | resolved IO1 level |
| 4 | `in0` | R | resolved IO0 level |
| 3 | `changed1` | R | `in1 XOR out1` |
| 2 | `changed0` | R | `in0 XOR out0` |
| 1 | `out1` | R/W | IO1 output latch |
| 0 | `out0` | R/W | IO0 output latch |

Timer0 output and codec TX can override the IO0 output latch. The `BP*`/`BG*`
instructions can transfer pin/codec endpoints without a general register move.
The VM resolves PIOC drive, external drive and internal pull-up into a physical
`0/1/Z/X` level; changing direction to input therefore models open-drain release.

## `codec.ctrl` (`0Ch`)

| Bit | Canonical role | Access | Meaning |
|---:|---|---|---|
| 7 | `tx_enable` | W | 0=receive/decode; 1=encode/transmit on IO0 |
| 6 | `manchester` | W | 0=25/75% PWM; 1=Manchester-style coding |
| 5 | `edge_sample` | W | select present midpoint or previous-edge sample |
| 4 | `done` | R | final quarter of bit cycle / codec completion |
| 3 | `count6` | R | internal cycle counter bit 6 |
| 2 | `count5` | R | internal cycle counter bit 5 |
| 1 | `count4` | R | internal cycle counter bit 4 |
| 0 | `count3` | R | internal cycle counter bit 3 |

The VM implements PWM and Manchester TX plus midpoint-based normal-waveform RX.
The manual does not fully specify receive phase and `edge_sample` boundaries, so
those exact boundaries remain subject to logic-analyzer calibration.

## `sys.ctrl` (`1Ch`)

| Bit | Canonical role | Controller | Meaning |
|---:|---|---|---|
| 7 | `irq_request` | PIOC | request host interrupt |
| 6 | `core_to_host_pending` | handshake | set by PIOC `cmd.out` write; host read clears |
| 5 | `host_to_core_pending` | handshake | set by host `cmd.in` write; PIOC read clears |
| 4 | `host_config` | host | application-defined configuration |
| 3 | `host_claim_io1` | host | 1 gives IO1 to PIOC, 0 to host |
| 2 | `host_claim_io0` | host | 1 gives IO0 to PIOC, 0 to host |
| 1 | `host_reset` | host | force PIOC reset |
| 0 | `host_clock_run` | host | run core; 0 gates clock and exposes SRAM |

The PDF description for bit 2 says IO1 in one place; its macro name, host
header and examples identify it as IO0.

## Command and shared registers

- Writing `cmd.out` sets `sys.ctrl.core_to_host_pending`; host reading it clears
  the pending bit. Legacy name `SFR_CTRL_RD` is from the host's read direction.
- Host writing `cmd.in` sets `sys.ctrl.host_to_core_pending`; PIOC reading it
  clears the pending bit. Legacy name `SFR_CTRL_WR` is from the host direction.
- `xchg` is ordinary shared data with special single-cycle bit-transfer paths.
- `d0..d31` have no hardware-prescribed meaning and are shared with the host.

Per the PIOC manual, byte `MOV`/`MOVA` accesses increment `ptr1` and therefore
use `[ptr1+]`; bit tests, bit set/clear and arithmetic accesses use `[ptr1]`
without increment. A bit operand is written `[ptr1][bit]`.
`MOVA2P` is the separate immediate-write-and-increment instruction.
