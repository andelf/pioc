# PIOC reference

This document describes the PIOC model implemented by the `pioc` workspace: the
16-bit instruction set, the cycle-aware virtual machine, the host interface, the
firmware command ABI, and the toolchain pipeline. It is the document to read
before changing the core, the parser, the VM, or the protocol drivers, and the
place to look up what a specific instruction or register does.

Where the WCH manual, its include files, and the EVT examples disagree, this
document says what the implementation does and marks the point as a calibration
item. The implementation target is the CH32X035 revision of the core
(`DecodeTarget::Ch32x035Pioc`); CH32X035 and CH32H417 have been exercised on
hardware, CH32V205 has a different firmware ABI and has not.

Companion documents:

- [syntax.md](syntax.md) — WCH legacy and canonical source syntax.
- [registers.md](registers.md) — per-bit register and bitfield detail.
- [../README.md](../README.md) — hardware inventory, install, CLI quick start.

Quick navigation:

- instruction semantics and encodings — section 3;
- execution timing, run states, waits, resets — section 4;
- register side effects and host handshake — sections 2.4 and 2.5;
- GPIO, Timer0, codec, wait events — section 5;
- host API, scenarios, trace and waveforms — section 6;
- firmware command ABI — section 7;
- known approximations and calibration points — section 9.

## 1. Machine model

| Property | Value |
|---|---|
| Instruction word | 16 bits |
| Program space | 2048 words (4 KiB) |
| Program SRAM | 4096 bytes; the same 2048 words addressed as bytes |
| Reset PC | `0x001`; word `0x000` is reserved and never executed |
| PC width | 11 bits, wrapping at `0x7ff` |
| Accumulator | 8 bits (`A`) |
| Flags | carry (`C`) and zero (`Z`) |
| Return stack | 6 entries; entries are typed (return address, accumulator+status, indirect-1 state) |
| Indirect address 0 | 9 bits (`ptr0`, `0x000..0x1ff`) |
| Indirect address 1 | 10 bits (`ptr1`, `0x000..0x3ff`) |
| Register file | 1024 bytes addressed by 10 bits; the core uses `0x00..0x3f` |
| Data pins | IO0 and IO1 |
| Timer | Timer0, 8-bit count/reload, divider `/1024` … `/1` |
| Codec | one PWM/Manchester channel on IO0 |

A `Machine` starts at PC `0x001`, with `A=0`, `C=0`, `Z=0`, an empty stack, and
all registers zero except `codec.ctrl=0x10` (tail set) and
`sys.ctrl=0x0d` (clock running, both pins granted to the core).

The loaded program may be shorter than 2048 words. Executing past its end is a
`VmError::ProgramCounter`, not an implicit zero fill. The CLI assembler pads to
2048 words only with `--full`, or as part of the host firmware ABI, which writes
payloads into the 4 KiB window.

### 1.1 Data accesses

The core has four ways to reach a byte:

| Form | Source | Address width | Behavior |
|---|---|---|---|
| Direct register | `R` in an instruction | 8 or 9 bits | reads/writes the register file |
| `[ptr0]` | register `0x00` | 9 bits via `ptr0` | indirect byte access, no increment |
| `[ptr1]` | register `0x01` | 10 bits via `ptr1` | indirect byte access; byte `MOV`/`MOVA` post-increment `ptr1` |
| Program byte | `code.read` | 11 bits | reads a program word, returns low byte in `A`, high byte into `ptr0` low byte |

`ptr0` and `ptr1` are registers `0x04` and `0x09`. Writing those registers from
the core replaces the low 8 bits and preserves the hidden high bits. The
immediate forms (`mov ptr0,#k`, `mov ptr1,#k`) replace the full pointer.

Only byte `MOV` and `MOVA` increment `ptr1`; `MOVA2P` is the separate
immediate-write-and-increment instruction. Bit access, bit set/clear, and
arithmetic register destinations use `[ptr1]` without incrementing.

### 1.2 Reset

A reset:

- sets PC to `0x001`, `A=0`, `C=0`, `Z=0`, and clears the stack;
- zeroes the register file, then sets `codec.ctrl=0x10`;
- preserves the host-owned `sys.ctrl` bits `0..4` (claim, reset, clock,
  host config);
- resets Timer0, codec phase, and indirect pointers;
- leaves the VM in `Running` when `sys.ctrl.host_clock_run=1`, otherwise in
  `ClockGated`.

Two events request a reset: a host write that sets `sys.ctrl.host_reset`, and
the Timer0 wait-timeout path (section 5.2). A reset is applied at the end of the
current peripheral tick; the step reports the cycles elapsed up to that point
and does not commit the pending `next_pc`. A host-requested reset then holds the
core in `ResetHeld` until the host clears the reset bit and selects a running
clock.

## 2. Registers

The 49 implemented SFRs are described below. [registers.md](registers.md) has
the full per-bit tables, reset values, and vendor-manual conflicts.

| Addr | Canonical | Core role |
|---:|---|---|
| `00` | `[ptr0]` | byte at `ptr0` |
| `01` | `[ptr1]` | byte at `ptr1`; byte `MOV`/`MOVA` post-increment |
| `02` | `pcl` | low 8 bits of PC; a write changes control flow |
| `03` | `status` | flags, stack state, timeout enable, GP bits |
| `04` | `ptr0` | indirect address 0 (low byte visible) |
| `05` | `timer0.count` | Timer0 count |
| `06` | `timer0.ctrl` | Timer0 control and pin-level wake |
| `07` | `timer0.reload` | Timer0 reload / PWM threshold |
| `08` | `codec.period` | codec period and TX bit |
| `09` | `ptr1` | indirect address 1 (low byte visible) |
| `0A` | `pins.dir` | direction, pull-ups, pad mode |
| `0B` | `pins` | inputs, changed/XOR bits, output latches, codec RX |
| `0C` | `codec.ctrl` | codec mode, done, counter bits |
| `0D..1B` | — | reserved; the VM retains writes and does not force zero |
| `1C` | `sys.ctrl` | handshake, IRQ, host ownership, reset/clock |
| `1D` | `cmd.out` | PIOC→host command byte |
| `1E` | `cmd.in` | host→PIOC command byte |
| `1F` | `xchg` | shared exchange byte |
| `20..3F` | `d0..d31` | shared data registers |

### 2.1 `status` (`03h`)

| Bit | Name | Owner | Behavior |
|---:|---|---|---|
| 7:6 | reserved | core storage | retained verbatim; not interpreted |
| 5 | `status.stack_used` | core | set when the typed stack is non-empty |
| 4 | `status.timeout_reset` | software | enables Timer0 overflow reset while waiting |
| 3 | `status.gp_y` | software | general-purpose bit |
| 2 | `status.zero` | core | zero flag |
| 1 | `status.gp_x` | software | general-purpose bit |
| 0 | `status.carry` | core | carry/borrow/shift flag |

`C` and `Z` live in separate VM fields during execution. Reading `status` first
copies them into bits 0 and 2, and the end of every step copies them back.
Writing `status` loads `C`/`Z` from bits 0 and 2; bits 7:6, 4, 3 and 1 keep the
written values. A host write to `status` stores the byte but does not change the
core's `C`/`Z` fields or its stack state; those bits are rewritten from core
state at the next core read or step.

Bit 4 is never set by the VM itself. Bit 5's position is contested between the
PDF and `PIOC_INC.ASM`; the VM follows the PDF and uses bit 4 for the timeout
enable. Treat timeout reset as a calibration item.

### 2.2 `pins.dir` (`0Ah`)

| Bits | Meaning |
|---|---|
| 7:4 | pad mode, not modeled |
| 3 / 2 | pull-up enable for IO1 / IO0 |
| 1 / 0 | direction for IO1 / IO0: 0 = input/high-Z, 1 = output |

A pin with no active driver resolves high when its pull-up bit is set,
otherwise high-Z.

### 2.3 `pins` (`0Bh`)

| Bit | Name | Access | Meaning |
|---:|---|---|---|
| 7 | `pins.input_xor` | R | `in0 XOR in1` |
| 6 | `codec.rx_bit` | R | codec RX sample |
| 5 | `pins.in1` | R | resolved IO1 level |
| 4 | `pins.in0` | R | resolved IO0 level |
| 3 | `pins.changed1` | R | `in1 XOR out1` |
| 2 | `pins.changed0` | R | `in0 XOR out0` |
| 1 | `pins.out1` | R/W | IO1 output latch |
| 0 | `pins.out0` | R/W | IO0 output latch |

Bits 7:2 are recomputed on every pin refresh. `changedN` is not an edge-latch;
it is the current input XOR the `pins.outN` latch. Edge detection for wait
events lives in private VM state.

### 2.4 `sys.ctrl` (`1Ch`) and handshake

| Bit | Name | Controller | Behavior |
|---:|---|---|---|
| 7 | `sys.irq_request` | PIOC | interrupt request; cleared by a host read or false write of `cmd.out` |
| 6 | `sys.ctrl.core_to_host_pending` | both | set when the core writes `cmd.out`; cleared when the host reads `cmd.out` |
| 5 | `sys.ctrl.host_to_core_pending` | both | set by a host write to `cmd.in`; cleared by a core read of `cmd.in` |
| 4 | `sys.ctrl.host_config` | host | application-defined; RGB1W uses it as the parasite-power flag |
| 3 / 2 | IO1 / IO0 claim | host | grants the pin to the PIOC; clear means the host owns it |
| 1 | `sys.ctrl.host_reset` | host | reset the core |
| 0 | `sys.ctrl.host_clock_run` | host | run the core; clear gates the clock |

Core writes to `sys.ctrl` may only change bits 7:5; host writes may only change
bits 4:0. The other side's bits are preserved.

Handshake rules, matching the vendor host code:

- Host writes `cmd.in`: the byte is stored and bit 5 is set.
- Core reads `cmd.in`: returns the byte and clears bit 5. Host reads of
  `cmd.in` do **not** clear it.
- Core writes `cmd.out`: the byte is stored and bit 6 is set.
- Host reads `cmd.out`: returns the byte and clears bits 6 and 7.
- Host writes `cmd.out`: data is ignored, only bit 7 is cleared. Vendor host
  code uses this "false write" to acknowledge an interrupt without consuming
  the byte.
- The core does not vector interrupts. `reti` is identical to `ret`; the IRQ
  request is only a host-visible flag.

### 2.5 Side effects of register access

Core reads and writes are not plain array accesses. The VM applies these rules:

| Access | Effect |
|---|---|
| read `[ptr0]` / `[ptr1]` | returns memory at the current pointer |
| write `[ptr0]` / `[ptr1]` | writes memory at the current pointer |
| read `pcl` | returns PC low byte |
| write `pcl` | replaces the low 8 bits of the next PC |
| read `status` | updates bits 0/2 from `C`/`Z`, then returns |
| write `status` | loads `C`/`Z` from bits 0/2 |
| read `pins` | refreshes resolved inputs first |
| read `cmd.in` | clears `sys.ctrl.host_to_core_pending` |
| write `cmd.out` | sets `sys.ctrl.core_to_host_pending` |
| write `sys.ctrl` (core) | may change bits 7:5 only |
| write `codec.period` | clears IO0 edge records; a period change stops the codec |
| write `codec.ctrl` (core) | writes bits 7:5, preserves read-only bits 4:0 |
| write `ptr0`/`ptr1` | replaces the low 8 bits, preserving hidden high bits |
| write any instruction that targets `pcl` | forces at least 2 cycles |

`Machine::register(address)` performs a core-style read, so it has the same side
effects, including clearing the host-to-core pending bit when reading `cmd.in`.

## 3. Instruction reference

Notation:

- `A` — accumulator; `C`, `Z` — flags.
- `R` — a directly addressed register; `R[b]` — one of its bits.
- `P0`, `P1` — `ptr0`/`ptr1`; `[P0]`, `[P1]` — the byte they address.
- `d=0` writes `A`; `d=1` writes `R`. Canonical spells these
  `op a,R` and `op R[,a]`.
- `→A` / `→R` shows the destination.
- `—` in the flag column means the instruction leaves the flag unchanged.
- Cycle counts include the instruction fetch (`step` returns them).
- `*` marks behavior specific to the CH32X035 PIOC decode target.

### 3.1 Control

| Canonical | WCH legacy aliases | Encoding | Operation | Flags | Cycles |
|---|---|---|---|---|---|
| `nop` | `NOP` | `0x0000..0x0003` | nothing | — | 1 |
| `clr a` | `CLRA` | `0x0004..0x0007` | `A←0` | `Z←1` | 1 |
| `wdt.clear` | `CLRWDT`, `WDT` | `0x0008..0x000b` | watchdog not modeled | — | 1 |
| `sleep` | `SLEEP`, `HALT` | `0x000c` | enter `Sleeping` | — | 1 |
| `sleep #k` | `SLEEPX k` | `0x000c..0x000f` | enter `Sleeping`; `k` retained | — | 1 |
| `wait e` | `WAITB e` | `0x0010..0x0017` | stall until event `e` | — | 1 + stall |
| `wait core_to_host_clear` | `WAITRD` | `0x0010` | event 0 | — | 1 + stall |
| `wait host_to_core_pending` | `WAITWR`, `WAITSPI` | `0x0014` | event 4 | — | 1 + stall |
| `code.read #k` | `RDCODE`, `RCODE k` | `0x0018..0x001b` | read program word selected by `A[2:0]`,`P0[7:0]` | — | 3 |
| `code.write` | `WRCODE` | `0x001c` (Generic decode only) | see target table; CH32X035 executes this word as `BCTC 0` | — | — |
| `exec #k` | `EXEC k` | `0x001c..0x001f` (Generic decode only) | see target table; CH32X035 executes these words as `BCTC 0..3` | — | — |
| `mov c,input` | `BCTC i` | `0x001c..0x001f`* | `C←` selected bit input | `C` | 1 |
| `xor c,pins.in0` | `BCTC 0` | `0x001c`* | `C← C XOR resolved IO0` | `C` | 1 |
| `push {a,flags}` | `PUSHAS`, `PUSH` | `0x0020..0x0023` | push `A` and `status` | — | 1 |
| `pop {a,flags}` | `POPAS`, `POP` | `0x0024..0x0027` | pop `A` and `status` | from `status` | 1 |
| `push {ptr1,page}` | `PUSHA2` | `0x0028..0x002b` | push 10-bit `P1` | — | 1 |
| `pop {ptr1,page}` | `POPA2` | `0x002c..0x002f` | pop 10-bit `P1` | — | 1 |
| `ret` | `RET`, `RETURN` | `0x0030..0x0033` | pop return address into PC | — | 2 |
| `ret z=1` | `RETZ`, `RETOK` | `0x0034..0x0037` | `Z←1`, then return | `Z` | 2 |
| `reti` | `RETIE`, `RETI` | `0x0038..0x003b` | return; no interrupt state change | — | 2 |
| `ret #k` | `RETL k`, `DB k` | `0x2000..0x20ff` | `A←k`, then return | — | 2 |
| `ret z=0,#k` | `RETLN k`, `RETER k` | `0x2100..0x21ff` | `A←k`, `Z←0`, then return | `Z` | 2 |

Control opcodes outside `0x0000..0x003b` and the transfer range
`0x0080..0x00ff` decode as `Unknown` and stop the VM.

`code.read` computes the word address as `((A & 7) << 8) | (P0 & 0xff)`,
returns the low byte in `A`, and writes the high byte into the low byte of
`P0`. The space selector is ignored: all spaces alias the single loaded image.
The stock `RDCODE` is space 0, `RCODE k` selects `k` in `0..3`.

`pop {a,flags}` restores the full `status` byte, then reloads `C`/`Z` from it;
the core re-derives `stack_used` at the end of the step.

### 3.2 Register and byte operations

The encoding is `000d oooo ffffffff`; `oooo` selects the operation and the
destination bit is bit 12. All operand addresses are 8 bits.

| Canonical | WCH legacy | Encoding | Operation | Flags | Cycles |
|---|---|---|---|---|---|
| `clr R` | `CLR f`, `CLRF f` | `0x0100 \| f` | `R←0` | `Z←1` | 1 |
| `mov R,a` | `MOVA F` | `0x1000 \| F` | `R←A` | — | 1 |
| `mov a,R` | `MOV F,A`, `MOVF F,A` | `0x0200 \| F` (`d=0`) | `A←R` | `Z←(R==0)` | 1 |
| `mov R,R` | `MOV F,F`, `MOVF F,F` | `0x1200 \| F` (`d=1`) | `R←R` (same register) | `Z←(R==0)` | 1 |
| `inc a,R` / `inc R` | `INC f,d`, `INCF f,d` | `0x04xx` / `0x14xx` | `A←R+1` / `R←R+1` | `Z←(v==0)` | 1 |
| `dec a,R` / `dec R` | `DEC f,d`, `DECF f,d` | `0x05xx` / `0x15xx` | `A←R-1` / `R←R-1` | `Z←(v==0)` | 1 |
| `inc.skipz a,R` / `inc.skipz R` | `INCSZ f,d`, `INCFSZ f,d` | `0x06xx` / `0x16xx` | `R+1`; skip next if zero | — | 1 / 2 |
| `dec.skipz a,R` / `dec.skipz R` | `DECSZ f,d`, `DECFSZ f,d` | `0x07xx` / `0x17xx` | `R-1`; skip next if zero | — | 1 / 2 |
| `swap.nibbles a,R` / `swap.nibbles R` | `SWAP f,d`, `SWAPF f,d` | `0x08xx` / `0x18xx` | rotate `R` by 4 | — | 1 |
| `and a,R` / `and R,a` | `AND f,d`, `ANDF f,d` | `0x09xx` / `0x19xx` | `A & R` | `Z` | 1 |
| `or a,R` / `or R,a` | `IOR f,d`, `IORF f,d` | `0x0axx` / `0x1axx` | `A \| R` | `Z` | 1 |
| `xor a,R` / `xor R,a` | `XOR f,d`, `XORF f,d` | `0x0bxx` / `0x1bxx` | `A ^ R` | `Z` | 1 |
| `add a,R` / `add R,a` | `ADD f,d`, `ADDF f,d` | `0x0cxx` / `0x1cxx` | `A + R` | `Z`, `C` | 1 |
| `sub a,R,a` / `sub R,a` | `SUB f,d`, `SUBF f,d` | `0x0dxx` / `0x1dxx` | `R - A` | `Z`, `C` borrow | 1 |
| `rolc a,R` / `rolc R` | `RCL f,d`, `RCLF f,d`, `RLF f,d` | `0x0exx` / `0x1exx` | rotate left through carry | `C` | 1 |
| `rorc a,R` / `rorc R` | `RCR f,d`, `RCRF f,d`, `RRF f,d` | `0x0fxx` / `0x1fxx` | rotate right through carry | `C` | 1 |

Details that matter when reading firmware:

- `mov R,R` is a real copy through the same register: it reads, writes back, and
  updates `Z`. It is not a no-op if `R` is `[ptr1]`: the byte is read and
  written, then `ptr1` increments.
- Only `mov a,[ptr1]`, `mov [ptr1],a` (and their legacy spellings) increment
  `ptr1`. Arithmetic and bit operations with `[ptr1]` do not.
- `inc.skipz` and `dec.skipz` do **not** update `Z`; they branch on their
  intermediate result.
- `swap.nibbles` and the rotates leave `Z` unchanged. The rotates update `C`
  from the bit shifted out.
- `sub` computes `R - A`, not `A - R`. `C` is the borrow.
- `C` is not touched by `and`, `or`, `xor`, `inc`, `dec`, `clr`, or `mov`.
- Any register-destination instruction that writes `pcl` takes 2 cycles; the
  write replaces the low 8 bits of the next PC.

### 3.3 Literal operations

| Canonical | WCH legacy | Encoding | Operation | Flags | Cycles |
|---|---|---|---|---|---|
| `mov ptr0,#k` | `MOVIP k` | `0x2200 \| k9` (Generic) | `P0←k9` | — | 1 |
| `mov ptr1,#k` | `MOVIA k` | `0x2400 \| k10` (Generic) | `P1←k10` | — | 1 |
| `mov pins.dir,#k` | `MOVA1F k` | `0x2300 \| k8`* | `pins.dir←k` | — | 1 |
| `mov pins,#k` | `MOVA2F k` | `0x2500 \| k8`* | `pins←k` (raw, including output bits) | — | 1 |
| `mov [ptr1+],#k` | `MOVA2P k` | `0x2600 \| k8`* | `[P1]←k`, `P1←P1+1` | — | 1 |
| `mov [ptr0],#k` | `MOVA1P k` | `0x2700 \| k8`* | `[P0]←k` | — | 1 |
| `mov a,#k` | `MOVL k` | `0x2800 \| k8` | `A←k` | — | 1 |
| `and a,#k` | `ANDL k` | `0x2900 \| k8` | `A←A&k` | `Z` | 1 |
| `or a,#k` | `IORL k`, `ORL k` | `0x2a00 \| k8` | `A←A\|k` | `Z` | 1 |
| `xor a,#k` | `XORL k` | `0x2b00 \| k8` | `A←A^k` | `Z` | 1 |
| `add a,#k` | `ADDL k` | `0x2c00 \| k8` | `A←A+k` | `Z`, `C` | 1 |
| `sub a,#k,a` | `SUBL k` | `0x2d00 \| k8` | `A←k-A` | `Z`, `C` borrow | 1 |
| `flags.add #k,a` | `CMPLN k` | `0x2e00 \| k8` | flags from `k + A`; `A` unchanged | `Z`, `C` | 1 |
| `flags.sub #k,a` | `CMPL k` | `0x2f00 \| k8` | flags from `k - A`; `A` unchanged | `Z`, `C` borrow | 1 |

`mov a,#k` and the other literal moves do not modify flags. `mov.lit a,#0` is
the assembler escape that keeps the `MOVL 0` encoding instead of emitting the
shorter `CLRA`.

`mov [ptr1+],#k` writes memory at the current `ptr1`, then increments it.
`mov [ptr0],#k` does not increment.

### 3.4 Bit operations

| Canonical | WCH legacy | Encoding | Operation | Flags | Cycles |
|---|---|---|---|---|---|
| `bclr R[b]` | `BC f,b`, `BCF f,b` | `0x4000 \| b<<8 \| f` | `R[b]←0` | — | 1 |
| `bset R[b]` | `BS f,b`, `BSF f,b` | `0x4800 \| b<<8 \| f` | `R[b]←1` | — | 1 |
| `skip clear,R[b]` | `BTSC f,b`, `BTFSC f,b` | `0x5000 \| b<<8 \| f` | skip next if `R[b]==0` | — | 1 / 2 |
| `skip set,R[b]` | `BTSS f,b`, `BTFSS f,b` | `0x5800 \| b<<8 \| f` | skip next if `R[b]==1` | — | 1 / 2 |
| `mov c,input` | `BCTC i` | see control table | `C←` bit input | `C` | 1 |
| `mov output,ptr0[b]` | `BP1F o,b` | `0x0080 \| o<<3 \| b` | bit `b` of `ptr0` → output endpoint | — | 1 |
| `mov output,xchg[b]` | `BP2F o,b` | `0x00a0 \| o<<3 \| b` | bit `b` of `xchg` → output endpoint | — | 1 |
| `mov ptr0[b],input` | `BG1F i,b` | `0x00c0 \| i<<3 \| b` | input endpoint → bit `b` of `ptr0` | — | 1 |
| `mov xchg[b],input` | `BG2F i,b` | `0x00e0 \| i<<3 \| b` | input endpoint → bit `b` of `xchg` | — | 1 |

Bit inputs and outputs are listed in section 5.4. Note that `ptr0` here is the
**value of the pointer register**, not the byte it addresses. `mov
output,ptr0[b]` reads `ptr0[b]`; `mov ptr0[b],input` modifies `ptr0` and keeps
its hidden high bit. The same rule applies to `xchg`. `BCTC` and the `B*F`
transfers use different endpoint numbering; see section 5.4 before substituting
one naming form for the other.

`bclr`/`bset` and the skip tests read the register through the normal path, so
they observe `[ptr1]` for address `0x01` and do not increment it.

### 3.5 Branches and calls

| Canonical | WCH legacy | Encoding | Condition / operation | Cycles |
|---|---|---|---|---|
| `jmp target` | `JMP k`, `GOTO k` | `0x6000 \| k12` | `PC←target & 0x7ff` | 2 |
| `call target` | `CALL k` | `0x7000 \| k12` | push return, `PC←target & 0x7ff` | 2 |
| `jmp nz,target` | `JNZ k` | `0x3000 \| k10` | take if `Z==0` | 1 / 2 |
| `jmp z,target` | `JZ k` | `0x3400 \| k10` | take if `Z==1` | 1 / 2 |
| `jmp nc,target` | `JNC k` | `0x3800 \| k10` | take if `C==0` | 1 / 2 |
| `jmp c,target` | `JC k` | `0x3c00 \| k10` | take if `C==1` | 1 / 2 |
| `jmp a == #k,t` | `CMPZ k7,t8` | `0x8000 \| k7<<8 \| t8` | take if `A==k7` | 1 / 2 |

Conditional branches do not replace the whole PC. The VM forms `next_pc =
(pc + 1)` and, when the condition holds, merges the encoded target:

| Instruction | Merge mask | Effective target |
|---|---|---|
| `jmp nz/z/nc/c` | `0x03ff` | low 10 bits from the instruction, bit 10 from `pc+1` |
| `jmp a == #k,t` | `0x00ff` | low 8 bits from the instruction, bits 10:8 from `pc+1` |

`jmp` and `call` replace the PC outright, masked to 11 bits. A `call` pushes the
already-incremented PC, so the return lands on the instruction after the call.

The skip instructions (`inc.skipz`, `dec.skipz`, `skip clear`, `skip set`) add 1
to the already-incremented PC, skipping one word.

### 3.6 Target-dependent encodings

Opcodes `0x001c..0x001f` and `0x23xx`, `0x25xx`, `0x26xx`, `0x27xx` have two
interpretations. `Instruction::decode` takes a `DecodeTarget`; the assembler
emits the CH32X035 meaning for its canonical/legacy PIOC spellings.

| Word | `DecodeTarget::Generic` | `DecodeTarget::Ch32x035Pioc` |
|---|---|---|
| `0x001c` | `code.write` (`WRCODE`) | `mov c,input` input 0 (`xor c,pins.in0` spelling) |
| `0x001d..0x001f` | `exec #1..#3` (`EXEC`) | bit input 1..3 |
| `0x23xx` | `mov ptr0,#0x1xx` (`MOVIP`) | `mov pins.dir,#k` (`MOVA1F`) |
| `0x25xx` | `mov ptr1,#0x1xx` (`MOVIA`) | `mov pins,#k` (`MOVA2F`) |
| `0x26xx` | `mov ptr1,#0x2xx` (`MOVIA`) | `mov [ptr1+],#k` (`MOVA2P`) |
| `0x27xx` | `mov ptr1,#0x3xx` (`MOVIA`) | `mov [ptr0],#k` (`MOVA1P`) |

Decoding always preserves `DecodedInstruction::word`, so tools can re-encode
the original bits regardless of interpretation. The VM uses CH32X035, so a
legacy `WRCODE`/`EXEC` word executes as the corresponding `BCTC` bit input; the
VM never sees `WriteCode` or `Exec` from its own decoder. It stops on `Unknown`
words.

### 3.7 Cycle summary

| Class | Cycles |
|---|---:|
| Ordinary ALU, move, bit, literal, indirect pointer | 1 |
| `code.read` | 3 |
| `jmp`, `call`, `ret`, `ret z=1`, `reti`, `ret #k`, `ret z=0,#k` | 2 |
| Taken conditional branch or skip | 2 |
| Not-taken branch or skip | 1 |
| Any instruction that writes `pcl` | 2 |
| `wait` while blocked | 1 to enter, then 0 per stalled step |
| `sleep` | 1 |

A multi-cycle instruction gives the peripherals one tick per elapsed cycle.

## 4. Virtual machine architecture

`pioc-vm` is a deterministic, cycle-stepped model. It does not run firmware in
a host thread or use wall-clock time; every effect is a function of the loaded
program, the register state, scheduled host/pin events, and the cycle counter.

### 4.1 State

| Field | Type | Purpose |
|---|---|---|
| `pc` | `u16` | 11-bit program counter |
| `accumulator` | `u8` | `A` |
| `carry`, `zero` | `bool` | authoritative flags |
| `cycles` | `u64` | elapsed PIOC clocks |
| `state` | `RunState` | running, waiting, sleeping, gated, reset |
| `program` | `Vec<u16>` | loaded words, extensible by host program-RAM writes |
| `registers` | `[u8; 0x400]` | register file |
| `indirect_address0/1` | `u16` | hidden 9/10-bit pointers |
| `stack` | `Vec<StackEntry>` | typed entries, at most 6 |
| `external_pins` | `[Option<bool>; 2]` | host/external drive; `None` = released |
| `resolved_pins` | `[Option<bool>; 2]` | electrical result; `None` = X/Z |
| `core_pin_outputs` | `[Option<bool>; 2]` | what the PIOC is driving |
| timer/codec fields | — | phase, output latch, latched bit |
| `trace` | `Vec<TraceEvent>` | append-only event log |

### 4.2 `step()` pipeline

1. If the state is `Sleeping`, `ClockGated`, or `ResetHeld`, return 0 cycles and
   do not tick peripherals.
2. If the state is `Waiting { event }`, evaluate the event. If not ready, return
   0 cycles. If ready, record whether the timeout timer was active, switch to
   `Running`, and (for a timeout) reload the timer count and divider phase.
3. Fetch the word at `pc`; an out-of-range PC is an error. Decode with
   `DecodeTarget::Ch32x035Pioc` and append an `Instruction` trace event.
4. Set `next_pc = (pc + 1) & 0x7ff`, `elapsed = 1`, and execute.
5. Advance peripherals once per elapsed cycle: tick Timer0, tick the codec,
   apply a pending wait-timeout reset, and record core pin changes.
6. If no reset occurred, commit `next_pc`; otherwise the reset PC stays.
7. Add elapsed cycles, synchronize `status`, and return
   `StepOutcome { cycles, state }`.

`run_steps(n)` repeatedly calls `step()` while the state is `Running`. It stops
on `Waiting`, `Sleeping`, `ClockGated`, or `ResetHeld`, so it never idles on its
own.

`step_or_idle()` is the convenience wrapper for waits that can be satisfied by
the model itself. If `step()` returns 0 cycles and the machine is waiting on the
codec or IO0 edges, or on a timeout timer, it advances one peripheral cycle and
reports one cycle. Host-driven waits still need an external event.

### 4.3 Run states

| State | `step()` behavior | Leaves the state when |
|---|---|---|
| `Running` | executes one instruction | always stays running unless an instruction changes it |
| `Waiting { event }` | 0 cycles until ready | the wait event becomes true |
| `Sleeping` | 0 cycles; `step()` does not tick peripherals | a resolved pin change sets a level-wake bit (see below) |
| `ClockGated` | 0 cycles | a host `sys.ctrl` write sets `host_clock_run` |
| `ResetHeld` | 0 cycles | a host `sys.ctrl` write clears `host_reset` and sets `host_clock_run` |

Sleeping is only exited by `refresh_pin_inputs`, which is called by external pin
changes, register reads, host writes, and peripheral ticks. A bare `step()`
never advances time in this state, but `run_scenario` and `idle_until` do tick
the timer and codec, so a timer-driven IO0 change can wake the core through a
level-wake bit. Callers that want a sleeping core to stay parked must not
advance peripheral time; callers that want a timed wake should use a scenario
event or a level-wake configuration.

### 4.4 Waits, timeouts, and interrupts

`wait e` is a stall instruction. The first execution consumes one cycle, leaves
`pc` on the same instruction, and enters `Waiting { e }`. A ready event is
consumed immediately and execution proceeds normally.

While waiting, time only advances when the caller idles the machine
(`step_or_idle`, `run_scenario`, `idle_until`). This is what allows a long
`wait` for a host event to fast-forward to the event's cycle.

Wait events 2 and 3 (`io0_fall`, `io0_rise`) are sticky edge flags set by
resolved-level transitions and cleared when consumed. Events 5–7 read the
current `pins` changed bits; they are level tests, not latches.

Timeout reset: if `status.timeout_reset` is set and Timer0 is disabled, Timer0
is still counted whenever the caller advances peripheral time. On overflow the
core resets instead of toggling the timer output. If the wait becomes ready
first, the timer count is reloaded and the divider phase restarts. This path
follows the PDF's bit assignment and remains subject to hardware calibration.

Interrupts are flags only. A resolved pin change with the corresponding
`timer0.ctrl.levelN_enable` bit set sets `sys.irq_request` and wakes a sleeping
core. There is no vector table and no interrupt stack frame; `reti` behaves
like `ret`.

### 4.5 Reset paths

| Trigger | Effect |
|---|---|
| Host writes `sys.ctrl.host_reset=1` | full core reset; state becomes `ResetHeld` |
| Timer0 timeout while waiting | full core reset applied at that cycle |
| Host clears `sys.ctrl.host_clock_run` | `ClockGated`; state is retained until the clock is re-enabled |
| Host sets `host_clock_run` from gated/reset-held | `Running` |

A full reset clears the stack and registers but preserves the host-owned
`sys.ctrl` low bits. It does not clear the external pin drive or the trace.

## 5. Peripheral model

### 5.1 GPIO electrical model

The resolved level of a pin is a ternary result:

| Core drive | External drive | Pull-up | Resolved |
|---|---|---|---|
| `0`/`1` | released | any | core value |
| released | `0`/`1` | any | external value |
| `0` | `1`, or `1` | `0` | `X` (conflict) |
| `0`/`1` | same value | any | that value |
| released | released | enabled | `1` |
| released | released | disabled | `Z`/unknown |

`X` and `Z` both map to `None` in `pin_level`, `pins.inN` reads them as `0`
for bit-input purposes, and the trace detail is `resolved_x` for a conflict.
The core drives a pin only when its `sys.ctrl` claim bit is set and its
`pins.dir` direction bit selects output. This is what the host-side
`reset_halted`/`run` calls establish: a claim bit grants the pin to the PIOC,
and clearing it releases the pin to the host.

IO0 output priority is: codec TX, then Timer0 output, then the `pins.out0`
latch. IO1 uses `pins.out1`. A change in the core's output is recorded at the
next peripheral tick, not at the instruction boundary.

`pins.changedN` is `inN XOR outN`; it is not a sticky edge flag. The codec RX
bit is maintained by the codec, and `pins.input_xor` is `in0 XOR in1`.

### 5.2 Timer0

`timer0.ctrl`:

| Bits | Meaning |
|---|---|
| 7 / 6 | level-wake enable for IO1 / IO0 |
| 5 | enable Timer0 |
| 4 | route timer/PWM output to IO0 |
| 3 | 0 = reload timer, 1 = free-running PWM |
| 2:0 | divider: 0=`/1024`, 1=`/256`, 2=`/64`, 3=`/16`, 4=`/8`, 5=`/4`, 6=`/2`, 7=`/1` |

Every tick increments a divider phase. When it reaches the divider, the timer
advances:

- **Reload mode:** `count←count+1`. On `0xff`, `count←reload` and the output
  toggles.
- **PWM mode:** `count←count+1`, wrapping to 0. The pin level is
  `count > reload`.

The period in core clocks is `divider × (256 - reload)` for reload mode. PWM is
a 256-step cycle with documented duty `(255 - reload)/256`.

The timer output is routed only to IO0. A level change on either pin with the
matching level-wake bit set raises `sys.irq_request` and wakes a sleeping core.

### 5.3 Codec

`codec.period` bits 6:0 select the bit period; bit 7 is the next TX bit.
`codec.ctrl` bit 7 selects TX vs RX, bit 6 selects Manchester vs 25/75% PWM, and
bit 5 selects the RX sample edge (the VM always samples at the midpoint).

The codec is active only when the period is at least 4. Each period:

- samples the TX bit at phase 0 and at every wrap;
- in PWM TX, drives IO0 high for `ceil(period/4)` cycles for a 0 or
  `3*ceil(period/4)` for a 1;
- in Manchester TX, drives high for the first half of a 1 and low for a 0;
- in RX, samples IO0 at `period/2`; PWM decodes the level, Manchester decodes
  whether the level differs from the phase-0 level;
- sets `codec.done` during the final quarter;
- shifts the phase into the read-only `codec.ctrl` counter bits.

The codec is IO0-only. The exact RX phase and edge-sample boundaries are a
calibration item; traces should be compared with a logic analyzer before
relying on them.

`codec.ctrl` writes from the core preserve bits 4:0 and update bits 7:5.
Writing `codec.period` clears the IO0 edge flags and stops the codec when the
period changes.

### 5.4 Wait events and bit endpoints

Wait events (`wait e`, `WAITB e`):

| Value | Canonical | Legacy symbol | Ready when |
|---:|---|---|---|
| 0 | `core_to_host_clear` | `WB_DATA_SW_MR_0` | `sys.ctrl.core_to_host_pending == 0` |
| 1 | `codec_done` | `WB_BIT_CYC_TAIL_1` | `codec.ctrl.done == 1` |
| 2 | `io0_fall` | `WB_PORT_I0_FALL` | sticky IO0 falling edge |
| 3 | `io0_rise` | `WB_PORT_I0_RISE` | sticky IO0 rising edge |
| 4 | `host_to_core_pending` | `WB_DATA_MW_SR_1` | `sys.ctrl.host_to_core_pending == 1` |
| 5 | `io1_changed` | `WB_PORT_XOR1_1` | `pins.changed1 == 1` |
| 6 | `io0_unchanged` | `WB_PORT_XOR0_0` | `pins.changed0 == 0` |
| 7 | `io0_changed` | `WB_PORT_XOR0_1` | `pins.changed0 == 1` |

Only events 2 and 3 are consumed; the others are level tests.

`BCTC` inputs (`mov c,input`) are the `BitInput` domain:

| Value | Canonical | Legacy | Source |
|---:|---|---|---|
| 0 | `xor c,pins.in0` | `BI_C_XOR_IN0` | `C XOR resolved IO0` |
| 1 | `codec.rx_bit` | `BI_BIT_RX_I0` | `pins` bit 6 |
| 2 | `pins.in0` | `BI_PORT_IN0` | resolved IO0 |
| 3 | `pins.in1` | `BI_PORT_IN1` | resolved IO1 |

`BP1F`/`BP2F` destinations and `BG1F`/`BG2F` sources are the `BitIo` domain:

| Value | Output spelling | Input spelling | Legacy | Endpoint |
|---:|---|---|---|---|
| 0 | `status.carry` | `status.carry` | `BIO_FLAG_C` / `BIO_FLAG_C` | `C` |
| 1 | `codec.tx_bit` | `codec.rx_bit` | `BO_BIT_TX_O0` / `BI_BIT_RX_I0` | codec bit |
| 2 | `pins.out0` | `pins.in0` | `BO_PORT_OUT0` / `BI_PORT_IN0` | IO0 |
| 3 | `pins.out1` | `pins.in1` | `BO_PORT_OUT1` / `BI_PORT_IN1` | IO1 |

The two domains are not interchangeable. `mov c,pins.in0` selects `BitInput`
value 2 (plain IO0); `mov ptr0[b],pins.in0` selects `BitIo` value 2. `BitInput`
value 0 (carry XOR IO0) has no `BitIo` equivalent and must be written
`xor c,pins.in0`; `BitIo` value 0 is the carry itself. `bitin[n]` and
`bitio[n]` are the numeric escapes for the two domains.

## 6. Host interface and scenarios

### 6.1 `Machine` API

| Method | Purpose |
|---|---|
| `Machine::new(&[u16])` | load a program; error if longer than 2048 words |
| `step()` | execute one instruction or report 0 cycles while stalled |
| `run_steps(n)` | step up to `n` times while `Running` |
| `step_or_idle()` | step, or advance one peripheral cycle for internally satisfiable waits |
| `register(addr)` | core-style register read (side effects apply) |
| `host_read(addr)` / `host_write(addr, value)` | host bus access |
| `set_external_pin(pin, value)` | drive a pin |
| `set_external_drive(pin, Option<bool>)` | drive or release a pin |
| `pin_level(pin)` | resolved level, or `None` for X/Z |
| `program()` | loaded words, including payload writes |
| `write_program_ram(offset, &[u8])` | little-endian byte write into the 4 KiB window |
| `trace()` / `take_trace()` | inspect or drain the event log |

Program-RAM writes address bytes, not words: offset `0x400` is word `0x200`.
The write extends the program vector with zeros and errors if the range exceeds
4096 bytes. It is a host operation; it does not require the clock to be gated in
the model, though real firmware should be halted.

### 6.2 Scenarios

`run_scenario(machine, events, limits)` applies a cycle-ordered event list.

```json
[
  {"cycle": 0,  "type": "host_write", "address": 30, "value": 1},
  {"cycle": 10, "type": "host_write", "address": 32, "value": 165},
  {"cycle": 20, "type": "pin", "pin": 0, "value": false},
  {"cycle": 30, "type": "pin", "pin": 0, "value": null},
  {"cycle": 40, "type": "host_read", "address": 29}
]
```

| Field | Meaning |
|---|---|
| `cycle` | event time; non-decreasing across the list |
| `type` | `host_write`, `host_read`, or `pin` |
| `address` | 8-bit register address for host events |
| `value` | byte for writes; boolean or `null` for pin events |
| `pin` | 0 or 1 |

Events with the same cycle are applied in list order. An event that falls
during a multi-cycle instruction is applied at the next instruction boundary.
If the machine is idle or waiting, the runner fast-forwards to the next event
cycle; `ClockGated`/`ResetHeld` time advances without ticking the core.

`RunLimits` defaults to 100,000 steps and 10,000,000 cycles. The report stops
with:

- `StepLimit` — `max_steps` instructions executed;
- `CycleLimit` — `max_cycles` cycles reached;
- `Blocked` — no runnable instruction and no event can make progress.

`executed_steps` counts instructions that consumed at least one cycle; the
first `wait` counts, stalled polls do not.

### 6.3 Trace and waveforms

Every trace event has `cycle`, `kind`, `pc`, `word`, `address`, `value`, and
`detail`. Kinds:

| Kind | Emitted for |
|---|---|
| `instruction` | each fetch; `detail` holds the decoded instruction |
| `register_read` / `register_write` | core register accesses |
| `host_read` / `host_write` | host bus accesses |
| `pin` | external drive, core output changes, and resolved changes |
| `state` | resets, with `detail` = `host_reset` or `wait_timeout_reset` |

Pin events use `detail` = `external`, `core`, `resolved`, or `resolved_x`.

`to_wavejson` produces WaveDrom lanes for core, external, and resolved IO0/IO1.
The `WaveOptions` defaults are window `0..256`, one slot per 2 cycles, and
48 MHz. The window must be non-empty, the quantum at least 1, and the result at
most 100,000 slots.
`to_vcd` emits only resolved levels on a 1 ps timescale, converting cycles with
the supplied clock frequency.

## 7. Firmware command ABI (`pioc-programs`)

`pioc-programs` models the CH32H417/CH32X035 RGB1W firmware, not the CH32V205
variant. `Firmware` carries the image words and the SFR-path bit period.

The host-side register block and program-SRAM base differ by family:

| Family | Register block | Program SRAM |
|---|---|---|
| CH32X0, CH643 | `0x4002_6C00` | `0x2000_4000` |
| CH32V205 | `0x4002_5C00` | `0x2000_0000` (low 4 KiB) |
| CH32H4 | `0x4002_5C00` | `0x5004_0000` |

`ch32-hal` implements the register-level `CommandPort`; the image words load at
program-SRAM offset 0, and RAM-path payloads load at byte `0x400`.

### 7.1 `CommandPort`

| Method | Effect |
|---|---|
| `reset_halted(pins)` | set claim bits for `pins`, assert reset, stop clock |
| `run(pins)` | set claim bits, release reset, run clock |
| `set_host_config(bool)` | set `sys.ctrl.host_config` |
| `write_data(offset, &[u8])` | write `d0..d31` |
| `read_data(offset, &mut [u8])` | read `d0..d31` |
| `write_program_ram(offset, &[u8])` | write payload bytes into the 4 KiB window |
| `program_ram_capacity()` | payload capacity reported by the backend |
| `start_command(u8)` | write `cmd.in` |
| `wait_for_interrupt()` | wait for IRQ and return `cmd.out` |

The trait is transport-neutral: the HAL provides the register-level
implementation, and the test suite provides a VM-backed one.

### 7.2 Commands and payloads

| Path | `cmd.in` | Payload | Notes |
|---|---|---|---|
| SFR | `len \| pin<<6` (IO0: `0x01..0x20`, IO1: `0x41..0x60`) | `d0..d31`, 1–32 bytes | short path; IO1 lengths 1–3 collide with `0x41..0x43` |
| Program RAM | `0x80 \| bit_cycles` | `d0..d1` = length LE, `d2` = output pin, payload at byte `0x400` | capacity 3072 bytes; bit cycles 8–124 |

Status bytes returned through `cmd.out`:

| Value | Meaning |
|---:|---|
| 0 | success |
| 1 | unknown command |
| 2 | invalid parameter |
| 4 | output high |
| 6 | input high |
| other | `FirmwareError::Other(value)` |

`bit_cycles_for_hz(hz)` rounds `1170 ns × hz` to the nearest PIOC clock and
returns `None` outside `8..=124` (28/56/117 at 24/48/100 MHz).

### 7.3 WS2812 selection

`Ws2812` chooses storage per transfer:

- `Storage::Sfr` requires length ≤ 32, no IO1 1–3-byte collision, matching
  `sfr_bit_cycles`, and no earlier RAM payload.
- `Storage::ProgramRam` requires a backend capacity and `bit_cycles` in
  `8..=124`.
- `Storage::Auto` uses SFR when all SFR conditions hold and falls back to RAM.

Once a RAM payload is written, the tail of the firmware image is overwritten;
the driver keeps `program_ram_dirty` set and rejects an explicit `Storage::Sfr`
afterwards with `SfrUnavailable`.

### 7.4 DS18x20

`Ds18x20` issues the firmware's 1-Wire commands:

| Command | Meaning |
|---|---|
| `0x41` | start conversion |
| `0x42` | read temperature |
| `0x43` | convert, wait 750 ms in the PIOC, read the first two scratchpad bytes |

`set_host_config` selects parasite power. Temperatures are returned as raw
signed 16-bit counts in 1/16 °C; `milli_celsius()` converts to m°C.

## 8. Toolchain architecture

### 8.1 Data flow

```text
 legacy .ASM ──┐                         ┌── listing / source (--source)
 canonical .pio┴─► parser ─► Program ────┤
                              words      └─► little-endian .bin
                                 │
                                 ├─► Instruction::decode(target)
                                 │
                                 └─► Machine ─► step ─► trace ─► JSONL
                                                        └─► WaveJSON / VCD

 .bin ─► Program::from_le_bytes ─► Machine
 .bin ─► Program::from_le_bytes ─► decode ─► printer
```

### 8.2 Crates

| Crate | Role | Key API |
|---|---|---|
| `pioc-core` | `no_std` instruction/register model | `Instruction`, `Register`, `Sfr`, `DecodeTarget`, `PROGRAM_WORDS`, `RESET_PC` |
| `pioc-parser` | two-pass assembler, translator, printers | `assemble_legacy`, `assemble_canonical`, `assemble_file*`, `translate_legacy*`, `Program`, `format_*` |
| `pioc-proc` | compile-time assembly | `pioc_asm!`, `pioc_asm_canonical!`, `pioc_file!`, `pioc_file_canonical!` |
| `pioc-vm` | machine, scenarios, waveforms | `Machine`, `run_scenario`, `RunLimits`, `to_wavejson`, `to_vcd` |
| `pioc-programs` | protocol drivers | `CommandPort`, `Firmware`, `Ws2812`, `Ds18x20` |
| `pioc` | CLI | `assemble`, `translate`, `disassemble`, `run` |

### 8.3 Assembler and disassembler

The parser is two-pass: pass one resolves labels, constants, `.origin`, and
include expansion; pass two emits words. Legacy and canonical front ends share
the same encoder, so a program's bytes depend only on the instructions, not the
dialect.

`translate_legacy` rewrites legacy source to canonical while preserving labels
and comments. `disassemble --canonical --source` emits reassemblable canonical
source; it uses `.word 0xNNNN` for unknown or non-canonical encodings so the
round trip is lossless. Address listings print `pc`, the raw word, and the
decoded instruction.

The process macros assemble at compile time relative to `CARGO_MANIFEST_DIR`.
They inject `include_str!` dependencies for every assembled file, so editing an
included source triggers a rebuild, and they report parse errors at the macro
call site.

### 8.4 CLI

| Command | Default output | Notes |
|---|---|---|
| `assemble input` | `input.bin` | legacy; `--canonical` selects canonical |
| `assemble --full` | `input.bin` | pad to 2048 words |
| `translate input` | `input.pio` | legacy → canonical |
| `disassemble input` | stdout | `--canonical`, `--source`, `-o` |
| `run input` | stdout report | `--scenario`, `--trace`, `--wavejson`, `--vcd`, limits, pin states |

`run` defaults: 100,000 steps, 10,000,000 cycles, 48 MHz waveform clock, a
2-cycle WaveJSON quantum, and a window ending at the final cycle.
Cycle-0 pin states accept `low`/`0`, `high`/`1`, and `z`/`release`.

## 9. What is intentionally not modeled

- `WRCODE` program writes: the PIOC manual says they are unsupported. The word
  encodes under Generic decode, but CH32X035 redefines the same range as
  `BCTC`, so the VM executes `0x001c` as a bit-input move, not a code write.
- Generic `EXEC` operations: the same redefinition applies to `0x001d..0x001f`.
  Only genuinely unknown words stop the VM.
- `RCODE` space selection: all spaces read the one loaded image.
- Reserved SFRs `0x0d..0x1b`: writes are retained, reads return what was
  stored.
- Analog behavior: rise/fall times, drive strength, and pad modes are not
  simulated.
- Codec RX phase and `edge_sample`: midpoint sampling is an approximation;
  calibrate against silicon.
- Same-cycle hardware priority between codec, Timer0, and the output latch:
  the model uses the documented priority (codec, then timer, then latch) and
  records changes at the peripheral tick.
- Interrupt entry/return: there is no vector table or interrupt frame.
- Sleeping with Timer0: a sleeping core only wakes from a resolved pin change
  with a level-wake bit. `step()` does not tick peripherals in `Sleeping`;
  `run_scenario` and `idle_until` do, so timed wake depends on the caller.
- CH32V205 firmware ABI: `0x70` RAM command, `0x600` payload offset, and the
  low 4 KiB program SRAM are not implemented.

The EVT golden tests and the canonical firmware tests are the executable
specification for the behaviors described here. When this document and a test
disagree, the test is the current truth and the document needs an update.
