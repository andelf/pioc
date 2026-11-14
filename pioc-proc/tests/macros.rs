const LEGACY: &[u16] = pioc_proc::pioc_asm!("ORG 0", "DW 0", "MOVL 0x55", "JMP 1");

const CANONICAL: &[u16] =
    pioc_proc::pioc_asm_canonical!(".origin 0", ".word 0", "mov.lit a,#0x55", "jmp 0x001",);
const FILE: &[u16] = pioc_proc::pioc_file!("tests/fixtures/program.asm");
const CANONICAL_FILE: &[u16] = pioc_proc::pioc_file_canonical!("tests/fixtures/program.pio");

#[test]
fn macros_assemble_at_compile_time() {
    assert_eq!(LEGACY, &[0x0000, 0x2855, 0x6001]);
    assert_eq!(CANONICAL, LEGACY);
    assert_eq!(FILE, &[0x0000, 0x285a]);
    assert_eq!(CANONICAL_FILE, &[0x0000, 0x005a]);
}
