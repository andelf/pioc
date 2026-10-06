# PIOC instruction set and registers

This document records the machine model used by the tools. Legacy names are
retained for vendor compatibility; canonical names make operand roles and
hardware domains explicit. Words are 16-bit and BIN files are little-endian.
Reset execution starts at word `0x001`; PIOC program space is 2048 words.

## Operand notation

| Field | Meaning |
|---|---|
| `A` | 8-bit accumulator |
| `f` / `F` | 8-bit / 9-bit directly addressed register |
| `d` | `0`: accumulator destination; `1`: source-register destination |
| `b` | bit index `0..7` |
| `a` | bit endpoint/input selector |
| `kN` | unsigned N-bit literal or target |

## Complete instruction map

An `x` bit is ignored by the base mnemonic. Some encodings have generic RISC8B
and CH32X035 PIOC interpretations; decoding is therefore target-aware and
always retains the original word.

### Control (18 documented spellings)

| Legacy spelling | Encoding | Semantics |
|---|---|---|
| `NOP` | `00000000 000000xx` | no operation |
| `CLRWDT` | `00000000 000010xx` | clear watchdog |
| `SLEEP` | `00000000 00001100` | sleep |
| `SLEEPX k2` | `00000000 000011kk` | parameterized sleep |
| `WAITB b` | `00000000 00010bbb` | wait for hardware event |
| `WAITRD` | `00000000 00010000` | alias of wait event 0 |
| `WAITWR` / `WAITSPI` | `00000000 00010100` | aliases of wait event 4 |
| `RDCODE` | `00000000 00011000` | read program word to `{SFR,A}` |
| `RCODE k2` | `00000000 000110kk` | read selected program space |
| `WRCODE` | `00000000 00011100` | program write; unsupported by PIOC manual |
| `EXEC k2` | `00000000 000111kk` | target-defined operation |
| `PUSHAS` | `00000000 001000xx` | push accumulator and status |
| `POPAS` | `00000000 001001xx` | restore accumulator and status |
| `PUSHA2` | `00000000 001010xx` | push indirect-address-2 state |
| `POPA2` | `00000000 001011xx` | restore indirect-address-2 state |
| `RET` | `00000000 001100xx` | return |
| `RETZ` | `00000000 001101xx` | return with `Z=1` |
| `RETIE` | `00000000 001110xx` | interrupt return |

### Register/byte operations (16)

| Legacy spelling | Encoding | Semantics / flags |
|---|---|---|
| `CLRA` | `00000000 000001xx` | `A=0`; Z |
| `CLR f` | `00000001 ffffffff` | `f=0`; Z |
| `MOVA F` | `0001000F FFFFFFFF` | `F=A` |
| `MOV F,d` | `000d001F FFFFFFFF` | copy `F`; Z |
| `INC f,d` | `000d0100 ffffffff` | `f+1`; Z |
| `DEC f,d` | `000d0101 ffffffff` | `f-1`; Z |
| `INCSZ f,d` | `000d0110 ffffffff` | increment; skip if zero |
| `DECSZ f,d` | `000d0111 ffffffff` | decrement; skip if zero |
| `SWAP f,d` | `000d1000 ffffffff` | swap nibbles |
| `AND f,d` | `000d1001 ffffffff` | `A & f`; Z |
| `IOR f,d` | `000d1010 ffffffff` | `A \| f`; Z |
| `XOR f,d` | `000d1011 ffffffff` | `A ^ f`; Z |
| `ADD f,d` | `000d1100 ffffffff` | `A + f`; Z,C |
| `SUB f,d` | `000d1101 ffffffff` | `f - A`; Z,C(borrow) |
| `RCL f,d` | `000d1110 ffffffff` | rotate left through carry |
| `RCR f,d` | `000d1111 ffffffff` | rotate right through carry |

### Literal operations (16)

| Legacy spelling | Encoding | Semantics |
|---|---|---|
| `RETL k8` | `00100000 kkkkkkkk` | `A=k`, return; `DB` uses this |
| `RETLN k8` | `00100001 kkkkkkkk` | `A=k`, `Z=0`, return |
| `MOVIP k9` | `0010001k kkkkkkkk` | load indirect address 0/page |
| `MOVIA k10` | `001001kk kkkkkkkk` | load indirect address 1/page |
| `MOVA1F k8` | `00100011 kkkkkkkk` | fast write `pins.dir` on PIOC |
| `MOVA2F k8` | `00100101 kkkkkkkk` | fast write `pins` on PIOC |
| `MOVA2P k8` | `00100110 kkkkkkkk` | write `[p1+]` |
| `MOVA1P k8` | `00100111 kkkkkkkk` | write `[p0]` |
| `MOVL k8` | `00101000 kkkkkkkk` | `A=k` |
| `ANDL k8` | `00101001 kkkkkkkk` | `A &= k`; Z |
| `IORL k8` | `00101010 kkkkkkkk` | `A \|= k`; Z |
| `XORL k8` | `00101011 kkkkkkkk` | `A ^= k`; Z |
| `ADDL k8` | `00101100 kkkkkkkk` | `A += k`; Z,C |
| `SUBL k8` | `00101101 kkkkkkkk` | `A = k - A`; Z,C(borrow) |
| `CMPLN k8` | `00101110 kkkkkkkk` | flags from `k + A` |
| `CMPL k8` | `00101111 kkkkkkkk` | flags from `k - A` |

### Bit operations (9)

| Legacy spelling | Encoding | Semantics |
|---|---|---|
| `BC f,b` | `01000bbb ffffffff` | clear register bit |
| `BS f,b` | `01001bbb ffffffff` | set register bit |
| `BTSC f,b` | `01010bbb ffffffff` | skip if bit clear |
| `BTSS f,b` | `01011bbb ffffffff` | skip if bit set |
| `BCTC a` | `00000000 000111aa` | selected input to carry |
| `BP1F a,b` | `00000000 100aabbb` | `p0[b]` to selected endpoint |
| `BP2F a,b` | `00000000 101aabbb` | `xchg[b]` to selected endpoint |
| `BG1F a,b` | `00000000 110aabbb` | endpoint to `p0[b]` |
| `BG2F a,b` | `00000000 111aabbb` | endpoint to `xchg[b]` |

### Control transfer (7)

| Legacy spelling | Encoding | Semantics |
|---|---|---|
| `JMP k12` | `0110kkkk kkkkkkkk` | absolute jump |
| `CALL k12` | `0111kkkk kkkkkkkk` | call |
| `JNZ k10` | `001100kk kkkkkkkk` | branch if `Z=0` |
| `JZ k10` | `001101kk kkkkkkkk` | branch if `Z=1` |
| `JNC k10` | `001110kk kkkkkkkk` | branch if `C=0` |
| `JC k10` | `001111kk kkkkkkkk` | branch if `C=1` |
| `CMPZ K7,k8` | `1KKKKKKK kkkkkkkk` | branch if `A==K7` |

Most instructions take one PIOC clock. `JMP`, `CALL` and returns take two;
taken conditional branches/skips take two; `RDCODE` takes three in the VM.

## Canonical vocabulary

Canonical output is not a cosmetic lower-case C header. It exposes data flow:

```text
mov d0,a
mov a,d0
sub a,d0,a
mov pins.dir,#0x01
bset pins.output0
wait host_to_core_pending
jmp nz,label
```

Disassembly emits numeric targets because machine code contains no labels.
`.word 0xNNNN` preserves unknown words. The assembler accepts `.origin`/`ORG`,
`.word`/`DW`, labels, constants and the vendor legacy directives.

Documented SFR fields use dotted operands such as `status.zero`,
`timer0.ctrl.enable`, `pins.dir.output0`, `pins.input0`,
`codec.ctrl.tx_enable`, and `sys.ctrl.host_to_core_pending`. Unknown or generic
register bits retain the explicit `register[index]` form.

## SFR address map

| Address | Canonical | Vendor name | Purpose |
|---:|---|---|---|
| `00` | `idata0` | `SFR_INDIR_PORT` | indirect data port 0 |
| `01` | `idata1` | `SFR_INDIR_PORT2` | indirect data port 1; `MOV`/`MOVA` post-increment |
| `02` | `pcl` | `SFR_PRG_COUNT` | program counter low byte |
| `03` | `status` | `SFR_STATUS_REG` | flags, stack/timeout and GP bits |
| `04` | `p0` | `SFR_INDIR_ADDR` | indirect address 0 |
| `05` | `timer0.count` | `SFR_TMR0_COUNT` | Timer0 count |
| `06` | `timer0.ctrl` | `SFR_TIMER_CTRL` | Timer0 and pin wake control |
| `07` | `timer0.reload` | `SFR_TMR0_INIT` | reload/PWM threshold |
| `08` | `codec.period` | `SFR_BIT_CYCLE` | codec period and TX bit |
| `09` | `p1` | `SFR_INDIR_ADDR2` | indirect address 1 |
| `0A` | `pins.dir` | `SFR_PORT_DIR` | direction, pull and mode |
| `0B` | `pins` | `SFR_PORT_IO` | inputs, XOR and outputs |
| `0C` | `codec.ctrl` | `SFR_BIT_CONFIG` | codec configuration/status |
| `0D..1B` | — | reserved | reads as zero; preserve on write |
| `1C` | `sys.ctrl` | `SFR_SYS_CFG` | host ownership, handshake, IRQ |
| `1D` | `cmd.out` | `SFR_CTRL_RD` | PIOC-to-host command byte |
| `1E` | `cmd.in` | `SFR_CTRL_WR` | host-to-PIOC command byte |
| `1F` | `xchg` | `SFR_DATA_EXCH` | bidirectional exchange byte |
| `20..3F` | `d0..d31` | `SFR_DATA_REG0..31` | shared data registers |

### Important bitfields

| Register | Bits | Meaning |
|---|---|---|
| `status` | `2=Z`, `1=GP_X`, `0=C` | stable definitions |
| `timer0.ctrl` | `7=level1`, `6=level0`, `5=enable`, `4=output`, `3=PWM`, `2:0=divider` | `/1024,/256,/64,/16,/8,/4,/2,/1` |
| `pins.dir` | `3=pull1`, `2=pull0`, `1=dir1`, `0=dir0` | 0=input, 1=output |
| `pins` | `7=in0^in1`, `6=codec RX`, `5:4=inputs`, `3:2=input^output`, `1:0=outputs` | Timer0/codec may override output 0 |
| `codec.ctrl` | `7=TX`, `6=mode`, `5=edge sample`, `4=tail`, `3:0=count[6:3]` | PWM/Manchester codec |
| `sys.ctrl` | `7=IRQ`, `6=core→host`, `5=host→core`, `4=config`, `3:2=ownership`, `1=reset`, `0=clock` | mixed ownership |

## Known source conflicts

The PDF and official `PIOC_INC.ASM` exchange `status` bits 5 and 4: one calls
bit 5 `stack_used` and bit 4 `wait_timeout_reset`, while the include file does
the reverse. No official example exercises either bit. The VM currently follows
the PDF for both stack state and its implemented WAIT timeout-reset path; the
selected bit position remains subject to hardware calibration.

The PDF access table also places host-swapped GP Y/X fields on the Timer0 count
row. Its prose and `PIOC_SFR.h` place those views at `timer0.ctrl[5:4]`; host
tooling should follow the header/prose interpretation.
