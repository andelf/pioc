# pioc

The PIOC(Programmable Protocol I/O Microcontroller) of CH32X035/4/3 and CH643.

## The PIOC Assembly Language

Note that the original assembly language definition replies heavily on `#define`/`equ` directives.

### SFRs

Using `$` prefix to denote SFRs.

```text
IP1 ; SFR_INDIR_PORT - Indirect Port 1
IP2 ; SFR_INDIR_PORT2 - Indirect Port 2
PC ; SFR_PRG_COUNT - Program Counter
SR ; SFR_STATUS_REG - Status Register
IA1 ; SFR_INDIR_ADDR - Indirect Address 1
TMRCNT ; SFR_TMR0_COUNT - Timer 0 Counter
TMRCTL ; SFR_TIMER_CTRL - Timer Control
TMRINIT ; SFR_TMR0_INIT - Timer 0 Initial Value
BITCYC ; SFR_BIT_CYCLE - Bit Cycle
IA2 ; SFR_INDIR_ADDR2 - Indirect Address 2
PDIR ; SFR_PORT_DIR - Port Direction
PIO ; SFR_PORT_IO - Port I/O
BITCFG ; SFR_BIT_CONFIG - Bit Configuration
SYSCFG ; SFR_SYS_CFG - System Configuration
CTLRD ; SFR_CTRL_RD - Control Read
CTLWR ; SFR_CTRL_WR - Control Write
DEXCH ; SFR_DATA_EXCH - Data Exchange
```

SR

```text
SR.TR ; SB_EN_TOUT_RST - Timeout Reset Enable
SR.SU ; SB_STACK_USED - Stack Used
SR.GY ; SB_GP_BIT_Y - General Purpose Bit Y
SR.Z ; SB_FLAG_Z - Zero Flag
SR.GX ; SB_GP_BIT_X - General Purpose Bit X
SR.C ; SB_FLAG_C - Carry Flag
```

TMRCTL

```text
TMRCTL.L1E ; SB_EN_LEVEL1 - Enable Level 1
TMRCTL.L0E ; SB_EN_LEVEL0 - Enable Level 0
TMRCTL.T0E ; SB_TMR0_ENABLE - Timer 0 Enable
TMRCTL.T0OE ; SB_TMR0_OUT_EN - Timer 0 Output Enable
TMRCTL.T0M ; SB_TMR0_MODE - Timer 0 Mode
TMRCTL.T0F2 ; SB_TMR0_FREQ2 - Timer 0 Frequency Bit 2
TMRCTL.T0F1 ; SB_TMR0_FREQ1 - Timer 0 Frequency Bit 1
TMRCTL.T0F0 ; SB_TMR0_FREQ0 - Timer 0 Frequency Bit 0
```

BITCYC

```text
BITCYC.TXO ; SB_BIT_TX_O0 - Bit Transmit Output 0
BITCYC.C6 ; SB_BIT_CYCLE_6 - Bit Cycle 6
BITCYC.C5 ; SB_BIT_CYCLE_5 - Bit Cycle 5
BITCYC.C4 ; SB_BIT_CYCLE_4 - Bit Cycle 4
BITCYC.C3 ; SB_BIT_CYCLE_3 - Bit Cycle 3
BITCYC.C2 ; SB_BIT_CYCLE_2 - Bit Cycle 2
BITCYC.C1 ; SB_BIT_CYCLE_1 - Bit Cycle 1
BITCYC.C0 ; SB_BIT_CYCLE_0 - Bit Cycle 0
```

PDIR

```text
PDIR.M3 ; SB_PORT_MOD3 - Port Mode 3
PDIR.M2 ; SB_PORT_MOD2 - Port Mode 2
PDIR.M1 ; SB_PORT_MOD1 - Port Mode 1
PDIR.M0 ; SB_PORT_MOD0 - Port Mode 0
PDIR.PU1 ; SB_PORT_PU1 - Port Pull-Up 1
PDIR.PU0 ; SB_PORT_PU0 - Port Pull-Up 0
PDIR.D1 ; SB_PORT_DIR1 - Port Direction 1
PDIR.D0 ; SB_PORT_DIR0 - Port Direction 0
```

PIO

```text
PIO.INXOR ; SB_PORT_IN_XOR - Port Input XOR
PIO.RXI ; SB_BIT_RX_I0 - Bit Receive Input 0
PIO.IN1 ; SB_PORT_IN1 - Port Input 1
PIO.IN0 ; SB_PORT_IN0 - Port Input 0
PIO.XOR1 ; SB_PORT_XOR1 - Port XOR 1
PIO.XOR0 ; SB_PORT_XOR0 - Port XOR 0
PIO.OUT1 ; SB_PORT_OUT1 - Port Output 1
PIO.OUT0 ; SB_PORT_OUT0 - Port Output 0
```

BITCFG

```text
BITCFG.TXE ; SB_BIT_TX_EN - Bit Transmit Enable
BITCFG.CMOD ; SB_BIT_CODE_MOD - Bit Code Mode
BITCFG.EDGE ; SB_PORT_IN_EDGE - Port Input Edge
BITCFG.TAIL ; SB_BIT_CYC_TAIL - Bit Cycle Tail
BITCFG.CC6 ; SB_BIT_CYC_CNT6 - Bit Cycle Count 6
BITCFG.CC5 ; SB_BIT_CYC_CNT5 - Bit Cycle Count 5
BITCFG.CC4 ; SB_BIT_CYC_CNT4 - Bit Cycle Count 4
BITCFG.CC3 ; SB_BIT_CYC_CNT3 - Bit Cycle Count 3
```

SYSCFG

```text
SYSCFG.INTQ ; SB_INT_REQ - Interrupt Request
SYSCFG.DSMR ; SB_DATA_SW_MR - Data Switch Master to RAM
SYSCFG.DMSR ; SB_DATA_MW_SR - Data Master to Switch RAM
SYSCFG.MCB4 ; SB_MST_CFG_B4 - Master Config Bit 4
SYSCFG.MIO1 ; SB_MST_IO_EN1 - Master I/O Enable 1
SYSCFG.MIO0 ; SB_MST_IO_EN0 - Master I/O Enable 0
SYSCFG.MRST ; SB_MST_RESET - Master Reset
SYSCFG.MCLKG ; SB_MST_CLK_GATE - Master Clock Gate
```

## Project Status

- [x] OpCodes and AST
- [ ] Assemler
- [x] Disassembler
- [x] Pretty Printer for Program
- [ ] proc-macro for inline PIOC program

## Documentation

[Official Support BBS: Link to PIOC.zip, The docs, assembler, and examples](https://www.wch.cn/bbs/thread-111140-1.html)

## Specification

- `RISC8B` instruction set, embedded [CH537]?
- Official Assembler: MCU CH53X ASSEMBLER
- 2K code ROM*(4KB in SRAM), 49 SFR, PWM timer/counter
- 2 I/O ports
- single cycle instruction(almost)
- max 48MHz clock

[CH537]: https://www.wch-ic.com/products/CH537.html
