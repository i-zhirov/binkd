//! Shell-style pattern matching, the port of `pmatch.c`.

const CTLESC: u8 = b'\\';

/// Returns the byte at the index, or 0 at the end of the slice.
///
/// The C code reads the terminating NUL byte. The helper gives the same
/// result for a slice.
fn get(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

/// Returns the slice from the index, or an empty slice at the end.
fn get_from(s: &[u8], i: usize) -> &[u8] {
    s.get(i..).unwrap_or(&[])
}

/// Returns true if the pattern matches the string.
///
/// The port follows `xpmatch()` of `pmatch.c` exactly. The bracket
/// expression does not use the case-insensitive flag.
pub fn xpmatch(pattern: &[u8], string: &[u8], ncase: bool) -> bool {
    let mut p = 0usize;
    let mut q = 0usize;

    loop {
        let c = get(pattern, p);
        p += 1;
        match c {
            0 => break,
            CTLESC => {
                let a = get(pattern, p);
                p += 1;
                if get(string, q) != a {
                    return false;
                }
                q += 1;
            }
            b'?' => {
                if get(string, q) == 0 {
                    return false;
                }
                q += 1;
            }
            b'*' => {
                let c2 = get(pattern, p);
                if c2 != CTLESC && c2 != b'?' && c2 != b'*' && c2 != b'[' {
                    loop {
                        let cur = get(string, q);
                        if cur == c2 {
                            break;
                        }
                        if ncase && cur.eq_ignore_ascii_case(&c2) {
                            break;
                        }
                        if cur == 0 {
                            return false;
                        }
                        q += 1;
                    }
                }
                loop {
                    if xpmatch(get_from(pattern, p), get_from(string, q), ncase) {
                        return true;
                    }
                    let cur = get(string, q);
                    q += 1;
                    if cur == 0 {
                        break;
                    }
                }
                return false;
            }
            b'[' => {
                // Find the closing bracket.
                let mut endp = p;
                if get(pattern, endp) == b'!' {
                    endp += 1;
                }
                let mut closed = false;
                loop {
                    if get(pattern, endp) == 0 {
                        break;
                    }
                    if get(pattern, endp) == CTLESC {
                        endp += 1;
                    }
                    endp += 1;
                    if get(pattern, endp) == b']' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    // No closing bracket: the '[' is a literal character.
                    let cur = get(string, q);
                    q += 1;
                    if ncase {
                        if !cur.eq_ignore_ascii_case(&b'[') {
                            return false;
                        }
                    } else if cur != b'[' {
                        return false;
                    }
                } else {
                    let mut invert = false;
                    if get(pattern, p) == b'!' {
                        invert = true;
                        p += 1;
                    }
                    let mut found = false;
                    let chr = get(string, q);
                    q += 1;
                    let mut c2 = get(pattern, p);
                    p += 1;
                    loop {
                        if c2 == CTLESC {
                            c2 = get(pattern, p);
                            p += 1;
                        }
                        if get(pattern, p) == b'-' && get(pattern, p + 1) != b']' {
                            p += 1;
                            if get(pattern, p) == CTLESC {
                                p += 1;
                            }
                            let hi = get(pattern, p);
                            if chr >= c2 && chr <= hi {
                                found = true;
                            }
                            p += 1;
                        } else if chr == c2 {
                            found = true;
                        }
                        c2 = get(pattern, p);
                        p += 1;
                        if c2 == b']' {
                            break;
                        }
                    }
                    if found == invert {
                        return false;
                    }
                }
            }
            _ => {
                let cur = get(string, q);
                q += 1;
                if ncase {
                    if !cur.eq_ignore_ascii_case(&c) {
                        return false;
                    }
                } else if cur != c {
                    return false;
                }
            }
        }
    }
    get(string, q) == 0
}

/// Case-insensitive `xpmatch()`.
pub fn pmatch_ncase(pattern: &[u8], string: &[u8]) -> bool {
    xpmatch(pattern, string, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal() {
        assert!(xpmatch(b"abc", b"abc", false));
        assert!(!xpmatch(b"abc", b"abd", false));
        assert!(xpmatch(b"", b"", false));
    }

    #[test]
    fn star_matches_the_rest() {
        assert!(xpmatch(b"*.pkt", b"12345.pkt", true));
        assert!(!xpmatch(b"*.pkt", b"12345.pkt.bad", true));
    }
}
