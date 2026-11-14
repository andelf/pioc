# pioc

Assembler, disassembler and virtual-machine runner for WCH's PIOC.

```console
$ pioc translate RGB1W.ASM -o rgb1w.pio
$ pioc assemble rgb1w.pio --canonical -o rgb1w.bin
$ pioc run rgb1w.bin --host-write 0x1e=0x03
```

See the [repository](https://github.com/andelf/pioc) for the ISA notes and the
host-side protocol drivers in `pioc-programs`.
