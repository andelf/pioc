use std::path::{Path, PathBuf};

use pioc_core::{DecodeTarget, Instruction};
use pioc_parser::{assemble_canonical, assemble_file, format_decoded, Syntax};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ch32x035-main/EVT/EXAM/PIOC")
}

#[test]
fn official_bins_round_trip_through_canonical_disassembly() {
    let root = corpus_root();
    if !root.exists() {
        return;
    }
    let bins = [
        "1_Wire/Asm/RGB1W.BIN",
        "PIOC_IIC/Asm/PIOC_IIC.BIN",
        "PIOC_NEC/Asm/PIOC_NEC.BIN",
        "PIOC_Single_Wire/Asm/PIOC_Single_Wire.BIN",
        "PIOC_UART/Ams/PIOC_UART.BIN",
    ];
    for bin in bins {
        let expected = std::fs::read(root.join(bin)).unwrap();
        let mut source = String::from(".origin 0\n");
        for pair in expected.chunks_exact(2) {
            let word = u16::from_le_bytes([pair[0], pair[1]]);
            let decoded = Instruction::decode(word, DecodeTarget::Ch32x035Pioc);
            source.push_str(&format_decoded(decoded, Syntax::Canonical));
            source.push('\n');
        }
        let assembled = assemble_canonical(&source)
            .unwrap_or_else(|error| panic!("canonical round trip failed for {bin}: {error}"));
        let actual = assembled.to_le_bytes();
        if actual != expected {
            let byte = actual
                .iter()
                .zip(&expected)
                .position(|(actual, expected)| actual != expected)
                .unwrap_or(actual.len().min(expected.len()));
            panic!(
                "canonical mismatch for {bin} at word 0x{:03x}: actual={:02x?}, expected={:02x?}",
                byte / 2,
                &actual[byte & !1..(byte & !1) + 2],
                &expected[byte & !1..(byte & !1) + 2]
            );
        }
    }
}

#[test]
fn official_examples_match_vendor_bins_byte_for_byte() {
    let root = corpus_root();
    if !root.exists() {
        eprintln!("skipping external WCH corpus at {}", root.display());
        return;
    }
    let cases = [
        ("1_Wire/Asm/RGB1W.ASM", "1_Wire/Asm/RGB1W.BIN"),
        ("PIOC_IIC/Asm/PIOC_IIC.ASM", "PIOC_IIC/Asm/PIOC_IIC.BIN"),
        ("PIOC_NEC/Asm/PIOC_NEC.ASM", "PIOC_NEC/Asm/PIOC_NEC.BIN"),
        (
            "PIOC_Single_Wire/Asm/PIOC_Single_Wire.ASM",
            "PIOC_Single_Wire/Asm/PIOC_Single_Wire.BIN",
        ),
        ("PIOC_UART/Ams/PIOC_UART.ASM", "PIOC_UART/Ams/PIOC_UART.BIN"),
    ];

    for (asm, bin) in cases {
        let assembled = assemble_file(root.join(asm)).unwrap_or_else(|error| {
            panic!("failed to assemble {asm}: {error}");
        });
        let expected = std::fs::read(root.join(bin)).unwrap();
        assert_eq!(
            assembled.to_le_bytes(),
            expected,
            "vendor mismatch for {asm}"
        );
    }
}
