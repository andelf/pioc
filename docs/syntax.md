# WCH legacy and canonical syntax

`pioc` accepts two assembly dialects for the same 16-bit instruction set:

- **WCH legacy** — the vendor `ASM` dialect used by the PIOC EVT examples.
  Uppercase mnemonics, bare immediates, `f,d` destination fields and vendor
  `SFR_*` register names.
- **Canonical** — the dialect `pioc` emits. Lowercase mnemonics, `#` immediates,
  explicit `a` destinations, semantic register names such as `cmd.in` and
  dotted bitfield names such as `status.zero`.

Both assemble to identical machine words. `pioc translate` converts a vendor
source to canonical form while keeping labels and comments, and
`pioc disassemble --canonical` emits canonical source that reassembles
byte-for-byte.

The parser is deliberately permissive about directives: `ORG`/`.origin`,
`DW`/`.word`, `.alias`, `.const`, `.define`, `.program` and `END`/`.end` are
accepted in either dialect. The tables below show the conventional spelling
for each side, not a hard syntax boundary.

Mnemonics, directives, register names and bit names are case-insensitive.
Labels and constants are stored upper-cased.

## Directives and line syntax

| Purpose | WCH legacy | Canonical | Notes |
|---|---|---|---|
| Program name | `.program name` | `.program name` | accepted and ignored |
| Include | `INCLUDE file` | — | file assembly only; `translate` expands includes |
| Constant | `NAME EQU expression` | `.const name = expression` or `.define name expression` | stored as a numeric symbol |
| Alias | — | `.alias name = register` | e.g. `.alias count = d0` |
| Origin | `ORG expression` | `.origin expression` | may only move forward |
| Data word | `DW expression` | `.word expression` | one 16-bit word |
| Byte pseudo-op | `DB expression` | `ret #expression` | `DB` is `RETL`, not a byte store |
| End | `END` | `.end` | stops both passes |
| Label | `name:` | `name:` | same |
| Comment | `; comment` | `; comment` | from `;` to end of line |
| Literal | `123`, `0x7b`, `0b1111011`, `B'1111011'`, `7bh` | same | decimal, hex, binary, `h` suffix |
| Expression | `a+b`, `a-b`, symbol names | same | only `+` and `-`; rightmost operator splits |

String/char literals are not part of the language.

## Operands and addressing

| Concept | WCH legacy | Canonical | Example |
|---|---|---|---|
| Immediate | bare number or symbol | `#` prefix | `MOVL 0x55` → `mov a,#0x55` |
| Accumulator destination | `A` or `0` in the `d` field | explicit `a` | `ADD F,A` → `add a,d0` |
| Register destination | `F` or `1` in the `d` field | source register name | `ADD F,F` → `add d0,a` |
| Direct register | vendor `SFR_*` name or number | semantic name | `SFR_CTRL_RD` → `cmd.out` |
| Shared data register | `SFR_DATA_REG0..31` | `d0..d31` | — |
| Indirect data 0 | `SFR_INDIR_PORT` / `0` | `[ptr0]` or `idata0` | `mov a,[ptr0]` |
| Indirect data 1 | `SFR_INDIR_PORT2` / `1` | `[ptr1+]` for byte moves, `[ptr1]` for bit access | `mov a,[ptr1+]` |
| Indirect address | `MOVIP` / `MOVIA` | `mov ptr0,#k` / `mov ptr1,#k` | — |
| Register bit | `f,b` operand pair | dotted `reg.field` or `reg[b]` | `BS SFR_STATUS_REG,2` → `bset status.zero` |
| Unknown word | `.word 0xNNNN` | `.word 0xNNNN` | exact encoding escape |

Canonical destination rules:

- `mov a,reg` loads `A`; `mov reg,reg` (the same register twice) selects the
  `d=1` destination-register encoding of `MOV`.
- `mov reg,a` is `MOVA`.
- Byte ALU operations use `op a,reg` for an accumulator destination, or
  `op reg` (preferred) / `op reg,reg` for a register destination.
- `sub a,reg,a` and `sub a,#k,a` keep the vendor operand order `f - A`.

## Instruction map

Operand ranges are the encoded widths. Addresses are word addresses in the
2048-word program space unless noted.

### Control

| WCH legacy | Canonical | Range | Notes |
|---|---|---|---|
| `NOP` | `nop` | — | |
| `CLRWDT`, `WDT` | `wdt.clear` | — | `WDT` is an alias |
| `SLEEP`, `HALT` | `sleep` | — | `HALT` is an alias |
| `SLEEPX k` | `sleep #k` | 0..3 | |
| `WAITB e` | `wait event` | 0..7 or name | see [wait events](#wait-events) |
| `WAITRD` | `wait core_to_host_clear` | — | event 0 |
| `WAITWR`, `WAITSPI` | `wait host_to_core_pending` | — | event 4 |
| `RDCODE` | `code.read` | — | program space 0 |
| `RCODE k` | `code.read #k` | 0..7 | selected program space |
| `WRCODE` | `code.write` | — | the PIOC manual says unsupported; it still encodes |
| `EXEC k` | `exec #k` | 0..7 | target-defined operation |
| `BCTC i` | `mov c,input` or `xor c,pins.in0` | 0..3 | see [bit endpoints](#bit-endpoints) |
| `PUSHAS`, `PUSH` | `push {a,flags}` | — | `PUSH` is an alias |
| `POPAS`, `POP` | `pop {a,flags}` | — | `POP` is an alias |
| `PUSHA2` | `push {ptr1,page}` | — | |
| `POPA2` | `pop {ptr1,page}` | — | |
| `RET`, `RETURN` | `ret` | — | |
| `RETZ`, `RETOK` | `ret z=1` | — | |
| `RETIE`, `RETI` | `reti` | — | |
| `DB k` | `ret #k` | 0..255 | legacy byte pseudo-op |

### Register and byte operations

`f` is an 8-bit direct address; `F`/`reg` is a 9-bit register address.

| WCH legacy | Canonical | Notes |
|---|---|---|
| `CLRA` | `clr a` | |
| `CLR f`, `CLRF f` | `clr reg` | |
| `MOVA F` | `mov reg,a` | |
| `MOV F,d`, `MOVF F,d` | `mov a,F` (d=`A`/`0`), `mov F,F` (d=`F`/`1`) | register form repeats the name |
| `INC f,d`, `INCF f,d` | `inc a,f` or `inc f` | |
| `DEC f,d`, `DECF f,d` | `dec a,f` or `dec f` | |
| `INCSZ f,d`, `INCFSZ f,d` | `inc.skipz a,f` or `inc.skipz f` | skip if result is zero |
| `DECSZ f,d`, `DECFSZ f,d` | `dec.skipz a,f` or `dec.skipz f` | skip if result is zero |
| `SWAP f,d`, `SWAPF f,d` | `swap.nibbles a,f` or `swap.nibbles f` | |
| `AND f,d`, `ANDF f,d` | `and a,f` or `and f,a` | |
| `IOR f,d`, `IORF f,d` | `or a,f` or `or f,a` | |
| `XOR f,d`, `XORF f,d` | `xor a,f` or `xor f,a` | |
| `ADD f,d`, `ADDF f,d` | `add a,f` or `add f,a` | |
| `SUB f,d`, `SUBF f,d` | `sub a,f,a` or `sub f,a` | computes `f - A` |
| `RCL f,d`, `RCLF f,d`, `RLF f,d` | `rolc a,f` or `rolc f` | rotate left through carry |
| `RCR f,d`, `RCRF f,d`, `RRF f,d` | `rorc a,f` or `rorc f` | rotate right through carry |

### Literal operations

| WCH legacy | Canonical | Range | Notes |
|---|---|---|---|
| `RETL k` | `ret #k` | 0..255 | |
| `RETLN k`, `RETER k` | `ret z=0,#k` | 0..255 | `A=k`, `Z=0`, return |
| `MOVIP k` | `mov ptr0,#k` | 0..511 | load indirect address 0 |
| `MOVIA k` | `mov ptr1,#k` | 0..1023 | load indirect address 1 |
| `MOVA1F k` | `mov pins.dir,#k` | 0..255 | fast write `pins.dir` |
| `MOVA2F k` | `mov pins,#k` | 0..255 | fast write `pins` |
| `MOVA2P k` | `mov [ptr1+],#k` | 0..255 | write `[ptr1+]` |
| `MOVA1P k` | `mov [ptr0],#k` | 0..255 | write `[ptr0]` |
| `MOVL k` | `mov a,#k` | 0..255 | `k=0` should be written `mov.lit a,#0` to keep `MOVL` |
| `ANDL k` | `and a,#k` | 0..255 | |
| `IORL k`, `ORL k` | `or a,#k` | 0..255 | `ORL` is an alias |
| `XORL k` | `xor a,#k` | 0..255 | |
| `ADDL k` | `add a,#k` | 0..255 | |
| `SUBL k` | `sub a,#k,a` | 0..255 | `A = k - A` |
| `CMPLN k` | `flags.add #k,a` | 0..255 | flags from `k + A` |
| `CMPL k` | `flags.sub #k,a` | 0..255 | flags from `k - A` |

### Bit operations

`b` is a bit index `0..7`.

| WCH legacy | Canonical | Notes |
|---|---|---|
| `BC f,b`, `BCF f,b` | `bclr reg.field` or `bclr reg[b]` | |
| `BS f,b`, `BSF f,b` | `bset reg.field` or `bset reg[b]` | |
| `BTSC f,b`, `BTFSC f,b` | `skip clear,reg.field` | skip if bit is clear |
| `BTSS f,b`, `BTFSS f,b` | `skip set,reg.field` | skip if bit is set |
| `BCTC i` | `mov c,input` | input 0 is written `xor c,pins.in0` |
| `BP1F o,b` | `mov output,ptr0[b]` | `[ptr0]` bit to endpoint |
| `BP2F o,b` | `mov output,xchg[b]` | `xchg` bit to endpoint |
| `BG1F i,b` | `mov ptr0[b],input` | endpoint to `[ptr0]` bit |
| `BG2F i,b` | `mov xchg[b],input` | endpoint to `xchg` bit |

### Branches and calls

| WCH legacy | Canonical | Range | Notes |
|---|---|---|---|
| `JMP k`, `GOTO k` | `jmp target` | 0..4095 | unconditional |
| `CALL k` | `call target` | 0..4095 | |
| `JNZ k` | `jmp nz,target` | 0..1023 | `Z=0` |
| `JZ k` | `jmp z,target` | 0..1023 | `Z=1` |
| `JNC k` | `jmp nc,target` | 0..1023 | `C=0` |
| `JC k` | `jmp c,target` | 0..1023 | `C=1` |
| `CMPZ k,t` | `jmp a == #k,t` | `k`: 0..127, `t`: 0..255 | compare and branch |

Canonical branch conditions need the space in `a == #...`; the target is a
single operand after the comma.

## Registers

| Address | WCH legacy | Canonical | Aliases | Purpose |
|---:|---|---|---|---|
| `00` | `SFR_INDIR_PORT` | `[ptr0]` | `idata0` | data at `ptr0` |
| `01` | `SFR_INDIR_PORT2` | `[ptr1]` / `[ptr1+]` | `idata1` | data at `ptr1`; byte moves use `[ptr1+]` |
| `02` | `SFR_PRG_COUNT` | `pcl` | | program counter low byte |
| `03` | `SFR_STATUS_REG` | `status` | | flags and general-purpose bits |
| `04` | `SFR_INDIR_ADDR` | `ptr0` | `p0` | indirect address 0 |
| `05` | `SFR_TMR0_COUNT` | `timer0.count` | | Timer0 count |
| `06` | `SFR_TIMER_CTRL` | `timer0.ctrl` | | Timer0 control |
| `07` | `SFR_TMR0_INIT` | `timer0.reload` | | reload / PWM threshold |
| `08` | `SFR_BIT_CYCLE` | `codec.period` | | codec period and TX bit |
| `09` | `SFR_INDIR_ADDR2` | `ptr1` | `p1` | indirect address 1 |
| `0A` | `SFR_PORT_DIR` | `pins.dir` | | direction, pull and mode |
| `0B` | `SFR_PORT_IO` | `pins` | | inputs, XOR and outputs |
| `0C` | `SFR_BIT_CONFIG` | `codec.ctrl` | | codec configuration and state |
| `1C` | `SFR_SYS_CFG` | `sys.ctrl` | | handshake, interrupt and host control |
| `1D` | `SFR_CTRL_RD` | `cmd.out` | | PIOC to host command byte |
| `1E` | `SFR_CTRL_WR` | `cmd.in` | | host to PIOC command byte |
| `1F` | `SFR_DATA_EXCH` | `xchg` | | exchange byte |
| `20..3F` | `SFR_DATA_REG0..31` | `d0..d31` | | shared data registers |

Numeric addresses are accepted in both dialects. The legacy assembler also
predefines the canonical names as symbols, so `mov a,cmd.in` assembles in
legacy source too; canonical source can likewise use `SFR_CTRL_WR`.

## Bit names

Legacy writes a bit as `f,b`; canonical uses the dotted names below (or
`reg[b]` for undocumented bits).

| Register | Bit | Canonical | Meaning |
|---|---:|---|---|
| `status` | 5 | `status.stack_used` | return stack is non-empty |
| `status` | 4 | `status.timeout_reset` | Timer0 overflow reset while waiting |
| `status` | 3 | `status.gp_y` | software general-purpose bit Y |
| `status` | 2 | `status.zero` | ALU zero flag |
| `status` | 1 | `status.gp_x` | software general-purpose bit X |
| `status` | 0 | `status.carry` | carry / borrow / shift |
| `timer0.ctrl` | 7 | `timer0.ctrl.level1_enable` | IO1 level wake |
| `timer0.ctrl` | 6 | `timer0.ctrl.level0_enable` | IO0 level wake |
| `timer0.ctrl` | 5 | `timer0.ctrl.enable` | run Timer0 |
| `timer0.ctrl` | 4 | `timer0.ctrl.output_enable` | route timer/PWM to IO0 |
| `timer0.ctrl` | 3 | `timer0.ctrl.pwm_mode` | 0=reload timer, 1=PWM |
| `timer0.ctrl` | 2..0 | `timer0.ctrl.divider2..divider0` | divider select |
| `codec.period` | 7 | `codec.period.tx_bit` | next raw TX bit |
| `codec.period` | 6..0 | `codec.period.period6..period0` | bit-cycle count |
| `pins.dir` | 7..4 | `pins.dir.mode3..mode0` | pin mode |
| `pins.dir` | 3 | `pins.dir.pull1` | IO1 pull |
| `pins.dir` | 2 | `pins.dir.pull0` | IO0 pull |
| `pins.dir` | 1 | `pins.dir1` | IO1 direction |
| `pins.dir` | 0 | `pins.dir0` | IO0 direction |
| `pins` | 7 | `pins.input_xor` | IO0 XOR IO1 input |
| `pins` | 6 | `codec.rx_bit` | codec RX bit |
| `pins` | 5 | `pins.in1` | IO1 input |
| `pins` | 4 | `pins.in0` | IO0 input |
| `pins` | 3 | `pins.changed1` | IO1 changed |
| `pins` | 2 | `pins.changed0` | IO0 changed |
| `pins` | 1 | `pins.out1` | IO1 output |
| `pins` | 0 | `pins.out0` | IO0 output |
| `codec.ctrl` | 7 | `codec.ctrl.tx_enable` | codec TX enable |
| `codec.ctrl` | 6 | `codec.ctrl.manchester` | Manchester mode |
| `codec.ctrl` | 5 | `codec.ctrl.edge_sample` | edge sample select |
| `codec.ctrl` | 4 | `codec.done` | codec transfer done |
| `codec.ctrl` | 3..0 | `codec.ctrl.count6..count3` | bit count high bits |
| `sys.ctrl` | 7 | `sys.irq_request` | interrupt request |
| `sys.ctrl` | 6 | `sys.ctrl.core_to_host_pending` | PIOC to host pending |
| `sys.ctrl` | 5 | `sys.ctrl.host_to_core_pending` | host to PIOC pending |
| `sys.ctrl` | 4 | `sys.ctrl.host_config` | host configuration bit |
| `sys.ctrl` | 3 | `sys.ctrl.host_claim_io1` | host owns IO1 |
| `sys.ctrl` | 2 | `sys.ctrl.host_claim_io0` | host owns IO0 |
| `sys.ctrl` | 1 | `sys.ctrl.host_reset` | core reset |
| `sys.ctrl` | 0 | `sys.ctrl.host_clock_run` | core clock run |

Migration aliases accepted by canonical assembly: `pins.dir.output0` and
`pins.dir.output1`, `pins.input0` and `pins.input1`, `pins.output0` and
`pins.output1`, `pins.codec_rx`, `codec.ctrl.tail` and
`sys.ctrl.interrupt_request`.

## Wait events

| Value | WCH legacy symbol | Canonical |
|---:|---|---|
| 0 | `WB_DATA_SW_MR_0` | `core_to_host_clear` |
| 1 | `WB_BIT_CYC_TAIL_1` | `codec_done` |
| 2 | `WB_PORT_I0_FALL` | `io0_fall` |
| 3 | `WB_PORT_I0_RISE` | `io0_rise` |
| 4 | `WB_DATA_MW_SR_1` | `host_to_core_pending` |
| 5 | `WB_PORT_XOR1_1` | `io1_changed` |
| 6 | `WB_PORT_XOR0_0` | `io0_unchanged` |
| 7 | `WB_PORT_XOR0_1` | `io0_changed` |

`WAITB` also accepts the raw value. Canonical `wait` accepts the names only.

## Bit endpoints

Inputs used by `BCTC`, `BG1F` and `BG2F`:

| Value | WCH legacy | Canonical |
|---:|---|---|
| 0 | `BI_C_XOR_IN0` | `mov c,pins.in0` is the `xor c,pins.in0` special case |
| 1 | `BI_BIT_RX_I0` | `codec.rx_bit` |
| 2 | `BI_PORT_IN0` | `pins.in0` |
| 3 | `BI_PORT_IN1` | `pins.in1` |

Outputs used by `BP1F` and `BP2F`:

| Value | WCH legacy | Canonical |
|---:|---|---|
| 0 | `BIO_FLAG_C` | `status.carry` |
| 1 | `BO_BIT_TX_O0` | `codec.tx_bit` |
| 2 | `BO_PORT_OUT0` | `pins.out0` |
| 3 | `BO_PORT_OUT1` | `pins.out1` |

`mov c,pins.in0` selects input 2. Input 0 (carry XOR IO0) has its own spelling,
`xor c,pins.in0`; `xor c,carry_xor_io0` is accepted as an alias.

## Migration and lossless escapes

- `p0`, `p1`, `idata0`, `idata1`, `pins.input0/1` and `pins.output0/1` remain
  accepted so older canonical sources keep assembling.
- `bitin[n]` and `bitio[n]` are numeric endpoint escapes for `mov c,bitin[n]`
  and `mov bitio[n],reg[b]`.
- `.word 0xNNNN` writes an exact machine word. Canonical disassembly uses it
  whenever the decoded instruction would not re-encode to the original word,
  for example when don't-care bits are non-zero or the word is unknown.
- `mov a,#0` selects the short `CLRA` encoding. `mov.lit a,#0` requests the
  distinct `MOVL 0` word; `pioc disassemble` emits `mov.lit a,#0x00` for it.
- `BCTC 0` and `mov c,pins.in0` are different instructions: the first is
  `BI_C_XOR_IN0` (value 0), the second is `BI_PORT_IN0` (value 2). The special
  `xor c,pins.in0` spelling selects value 0.

## Worked example

Vendor source:

```asm
ORG 0
DW 0
START: MOVL 0x12
       MOVA 0x20
       JNZ START
       END
```

Canonical source:

```pio
.origin 0
.word 0
start: mov a,#0x12
       mov d0,a
       jmp nz,start
```

Both assemble to `00 00 12 28 20 10 01 30` (little-endian bytes). Run:

```console
pioc translate program.ASM -o program.pio
pioc assemble --canonical program.pio -o program.bin
```

[`docs/reference.md`](reference.md) documents the semantics and encodings of
every instruction, the VM architecture, and the host ABI;
[`docs/registers.md`](registers.md) documents the SFRs and bitfields in full.
