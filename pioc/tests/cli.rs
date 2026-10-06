use std::path::PathBuf;
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("pioc-cli-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn source_disassembly_reassembles_exactly() {
    let directory = scratch("roundtrip");
    let input = directory.join("input.bin");
    let source = directory.join("output.pio");
    let output = directory.join("output.bin");
    let expected = [0x00, 0x00, 0x55, 0x28, 0x01, 0x60];
    std::fs::write(&input, expected).unwrap();

    assert!(Command::new(env!("CARGO_BIN_EXE_pioc"))
        .args(["disassemble", "--canonical", "--source"])
        .arg(&input)
        .args(["-o"])
        .arg(&source)
        .status()
        .unwrap()
        .success());
    assert!(Command::new(env!("CARGO_BIN_EXE_pioc"))
        .args(["assemble", "--canonical"])
        .arg(&source)
        .args(["-o"])
        .arg(&output)
        .status()
        .unwrap()
        .success());
    assert_eq!(std::fs::read(output).unwrap(), expected);
}

#[test]
fn full_image_is_4096_bytes() {
    let directory = scratch("full");
    let input = directory.join("input.asm");
    let output = directory.join("output.bin");
    std::fs::write(&input, "ORG 0\nDW 0\nNOP\nEND\n").unwrap();

    assert!(Command::new(env!("CARGO_BIN_EXE_pioc"))
        .args(["assemble", "--full"])
        .arg(&input)
        .args(["-o"])
        .arg(&output)
        .status()
        .unwrap()
        .success());
    assert_eq!(std::fs::metadata(output).unwrap().len(), 4096);
}
