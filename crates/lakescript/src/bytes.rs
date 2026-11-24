//! Hex and base64 for the `bytes` type *(see `ideas/stdlib/types/bytes.md`)*.

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Bytes written in hex, in any case, with or without spaces *(i.e. `ff 00`)*.
pub fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    let digits: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !digits.len().is_multiple_of(2) {
        return Err(format!("'{text}' isn't hex: it has an odd number of digits"));
    }
    digits
        .chunks(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).unwrap_or("");
            u8::from_str_radix(pair, 16).map_err(|_| format!("'{text}' isn't hex: `{pair}` isn't a byte"))
        })
        .collect()
}

/// Standard base64, with `=` at the end, like `base64` and most APIs.
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Base64, standard or for URLs *(`-` and `_`)*, with or without `=`, and
/// with line breaks ignored.
pub fn from_base64(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut bits, mut count) = (0u32, 0);
    for c in text.chars().filter(|c| !c.is_whitespace()) {
        if c == '=' {
            break;
        }
        let value = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            _ => return Err(format!("'{text}' isn't base64: `{c}` can't be in it")),
        };
        bits = bits << 6 | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count & 0xff) as u8);
        }
    }
    Ok(out)
}
