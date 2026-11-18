//! The base64 codec of the proxy support.
//!
//! The port of `enbase64()` of `https.c` and of `enbase64()` and `debase64()` of
//! `ntlm/helpers.c`. The three functions hold the same alphabet of RFC 4648.

/// The alphabet of the C code, `b64t`.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Returns the base64 text of the bytes, the port of `enbase64()`.
pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() {
            data[i + 1] as u32
        } else {
            0
        };
        let b2 = if i + 2 < data.len() {
            data[i + 2] as u32
        } else {
            0
        };
        let c = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((c >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((c >> 12) & 0x3f) as usize] as char);
        // The C code writes the third and the fourth character of the last group
        // and replaces them with the padding.
        if i + 1 < data.len() {
            out.push(ALPHABET[((c >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < data.len() {
            out.push(ALPHABET[(c & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

/// Returns the bytes of the base64 text, the port of `debase64()`.
///
/// The C code stops at the first character that is not in the alphabet, so a
/// padding character ends the text. The C code reads the NUL terminator of the
/// alphabet for a NUL of the text; the port stops at the NUL.
pub fn decode(text: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut j = 0usize;
    for (i, &byte) in text.iter().enumerate() {
        let Some(k) = ALPHABET.iter().position(|&a| a == byte) else {
            break;
        };
        let k = k as u8;
        match i % 4 {
            0 => set(&mut out, j, k << 2),
            1 => {
                out[j] |= k >> 4;
                j += 1;
                set(&mut out, j, (k << 4) & 0xf0);
            }
            2 => {
                out[j] |= k >> 2;
                j += 1;
                set(&mut out, j, (k << 6) & 0xc0);
            }
            _ => {
                out[j] |= k;
                j += 1;
            }
        }
    }
    out.truncate(j);
    out
}

/// Writes a byte at an index of the result of `decode()`.
fn set(out: &mut Vec<u8>, j: usize, value: u8) {
    if out.len() <= j {
        out.resize(j + 1, 0);
    }
    out[j] = value;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_the_padding() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn decode_the_padding() {
        assert_eq!(decode(b""), b"");
        assert_eq!(decode(b"Zg=="), b"f");
        assert_eq!(decode(b"Zm8="), b"fo");
        assert_eq!(decode(b"Zm9v"), b"foo");
        assert_eq!(decode(b"Zm9vYg=="), b"foob");
        assert_eq!(decode(b"Zm9vYmFy"), b"foobar");
        // The C code stops at the first character outside the alphabet.
        assert_eq!(decode(b"Zm9v\r\n"), b"foo");
        assert_eq!(decode(b"Zm9v\0rest"), b"foo");
    }

    #[test]
    fn a_round_trip() {
        let text = b"user:password";
        assert_eq!(encode(text), "dXNlcjpwYXNzd29yZA==");
        assert_eq!(decode(b"dXNlcjpwYXNzd29yZA=="), text);
    }
}
