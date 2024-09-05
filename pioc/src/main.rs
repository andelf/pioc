use pioc::OpCode;

fn main() {
    let arg = std::env::args().nth(1).unwrap();

    let raw = std::fs::read(arg).unwrap();

    let mut program = vec![];
    for (i, word) in raw.chunks(2).enumerate() {
        let word = u16::from_le_bytes([word[0], word[1]]);
        let opcode = OpCode::from_word(word);
        println!("{}: 0x{:04X} {:?}", i, word, opcode);

        program.push(opcode);
    }

    for (loc, op) in program.iter().enumerate() {
        println!(".L{}\t\t{}", loc, op.to_wch_risc8b_asm());
    }
}
