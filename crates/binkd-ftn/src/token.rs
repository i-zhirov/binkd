//! Token parsing with quotes and escapes, the port of `getw.c`.

/// Substitute `%VAR%` with the environment value.
pub const GWX_SUBST: u32 = 1;
/// Stop at a `#` comment.
pub const GWX_HASH: u32 = 2;
/// Treat `\` as a regular character.
pub const GWX_NOESC: u32 = 4;

/// Returns the byte at the index, or 0 at the end of the slice.
fn get(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

/// Returns true for the whitespace of `isspace()` in the C locale.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// Returns the word number `n` from the source.
///
/// The port follows `getwordx2()` of `getw.c`. The C code can
/// under-allocate the buffer for a `%VAR%` expansion. The port returns
/// a correct value (see `doc/rust-rewrite-plan.md`).
pub fn getwordx2(
    src: Option<&[u8]>,
    n: i32,
    flags: u32,
    fldsep: &[u8],
    fldskip: &[u8],
) -> Option<Vec<u8>> {
    let src = src?;
    if n <= 0 {
        // The C code does not stop for n <= 0.
        return None;
    }

    let mut si = 0usize;
    let mut quoted = false;
    let mut remaining = n;

    loop {
        while si < src.len() && fldskip.contains(&src[si]) {
            si += 1;
        }
        if si >= src.len() || ((flags & GWX_HASH) != 0 && src[si] == b'#') {
            return None;
        }

        let mut word: Vec<u8> = Vec::with_capacity(src.len());
        loop {
            let c = get(src, si);
            if (flags & GWX_NOESC) == 0 && c == b'\\' {
                si += 1;
                let e = get(src, si);
                if !e.is_ascii_digit() && e != b'x' && e != b'X' {
                    let ch = match e {
                        b'a' => 0x07,
                        b'b' => 0x08,
                        b'f' => 0x0c,
                        b'n' => 0x0a,
                        b'r' => 0x0d,
                        b't' => 0x09,
                        b'E' => 0x1b,
                        other => other,
                    };
                    word.push(ch);
                    si += 1;
                } else {
                    let mut base = 8u32;
                    let mut ch = 0u32;
                    if e.eq_ignore_ascii_case(&b'X') {
                        si += 1;
                        base = 16;
                    }
                    while si < src.len() && src[si].is_ascii_hexdigit() {
                        let d = src[si];
                        if d.is_ascii_digit() {
                            ch = ch * base + (d - b'0') as u32;
                        } else if base == 16 {
                            ch = ch * base + (d.to_ascii_lowercase() - b'a') as u32 + 10;
                        } else {
                            break;
                        }
                        si += 1;
                    }
                    word.push(ch as u8);
                }
                continue;
            }
            if c == 0 || c == b'\n' || c == b'\r' {
                break;
            }
            if c == b'"' {
                quoted = !quoted;
                si += 1;
                continue;
            }
            if fldsep.contains(&c) && !quoted {
                break;
            }
            if c == b'%' && (flags & GWX_SUBST) != 0 {
                si += 1;
                if get(src, si) == b'%' {
                    word.push(b'%');
                    si += 1;
                } else {
                    let mut name = Vec::new();
                    while si < src.len() && !is_space(src[si]) && src[si] != b'%' {
                        name.push(src[si].to_ascii_uppercase());
                        si += 1;
                    }
                    let var = std::env::var(String::from_utf8_lossy(&name).into_owned())
                        .unwrap_or_default();
                    word.extend_from_slice(var.as_bytes());
                    // The C loop skips the closing '%'.
                    si += 1;
                }
                continue;
            }
            word.push(c);
            si += 1;
        }

        remaining -= 1;
        if remaining == 0 {
            return Some(word);
        }
        if si < src.len() && fldsep.contains(&src[si]) {
            si += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        assert_eq!(
            getwordx2(Some(b"hello world"), 2, 0, b" ", b" "),
            Some(b"world".to_vec())
        );
        assert_eq!(getwordx2(Some(b"hello world"), 3, 0, b" ", b" "), None);
    }

    #[test]
    fn quotes() {
        assert_eq!(
            getwordx2(Some(b"\"hello world\" tail"), 1, 0, b" ", b" "),
            Some(b"hello world".to_vec())
        );
    }
}
