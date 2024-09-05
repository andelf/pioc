use crate::{OpCode, U2};

pub fn parse_line(line: &str) -> Result<OpCode, String> {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();
    if parts.is_empty() {
        return Err("Empty line".to_string());
    }

    match parts[0].to_uppercase().as_str() {
        "NOP" => Ok(OpCode::Nop),
        "CLRWDT" => Ok(OpCode::ClearWatchDog),
        "SLEEPX" => {
            if parts.len() != 2 {
                return Err("SLEEPX requires one argument".to_string());
            }
            let k2 = parse_u2(parts[1])?;
            Ok(OpCode::Sleep(k2))
        }
        // ... Add more instructions here ...
        _ => Err(format!("Unknown instruction: {}", parts[0])),
    }
}

fn parse_u2(s: &str) -> Result<U2, String> {
    let value: u8 = s.parse().map_err(|_| "Invalid U2 value".to_string())?;
    U2::new(value).ok_or_else(|| "U2 value must be 0, 1, 2, or 3".to_string())
}
