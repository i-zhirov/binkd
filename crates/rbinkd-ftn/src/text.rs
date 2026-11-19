//! Pure string helpers of `tools.c`.

use crate::pattern::pmatch_ncase;

/// Quote control characters.
pub const SQ_CNTRL: i32 = 1;
/// Quote spaces.
pub const SQ_SPACE: i32 = 2;

/// Returns true for the whitespace of `isspace()` in the C locale.
pub fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// Returns the quoted form of the string.
///
/// The function quotes control characters, spaces, and the backslash with
/// the `\xNN` form.
pub fn strquote(s: &[u8], flags: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 4);
    for &b in s {
        if ((flags & SQ_CNTRL) != 0 && b.is_ascii_control())
            || ((flags & SQ_SPACE) != 0 && is_space(b))
            || b == b'\\'
        {
            out.extend_from_slice(format!("\\x{b:02x}").as_bytes());
        } else {
            out.push(b);
        }
    }
    out
}

/// Returns the unquoted form of the string.
///
/// The function reads the `\xHH` and `\HHH` forms of `strquote()`.
pub fn strdequote(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0usize;
    while i < s.len() {
        if s[i] == b'\\'
            && s.get(i + 1) == Some(&b'x')
            && is_hex(s.get(i + 2))
            && is_hex(s.get(i + 3))
        {
            out.push(hex_value(s[i + 2]) * 16 + hex_value(s[i + 3]));
            i += 4;
        } else if s[i] == b'\\' && is_hex(s.get(i + 1)) && is_hex(s.get(i + 2)) {
            out.push(hex_value(s[i + 1]) * 16 + hex_value(s[i + 2]));
            i += 3;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

fn is_hex(c: Option<&u8>) -> bool {
    c.is_some_and(|&c| c.is_ascii_hexdigit())
}

fn hex_value(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        _ => c.to_ascii_lowercase() - b'a' + 10,
    }
}

/// Replaces suspicious characters with `_` in place.
pub fn strwipe(s: &mut [u8]) {
    for b in s.iter_mut() {
        if b.is_ascii_control() || *b == b'\\' || *b == b'/' || *b == b':' {
            *b = b'_';
        }
    }
}

/// Returns the string with every entry of `a` replaced by `b`.
///
/// The compare ignores case. The C code grows the buffer and can copy the
/// source onto itself for an empty `a`. The port returns a new value.
pub fn ed(s: Option<&[u8]>, a: Option<&[u8]>, b: Option<&[u8]>) -> Vec<u8> {
    let a = a.unwrap_or(&[]);
    let b = b.unwrap_or(&[]);
    let Some(s) = s else {
        return Vec::new();
    };
    if a.is_empty() {
        return s.to_vec();
    }

    let len_a = a.len();
    let len_b = b.len();
    let mut r: Vec<u8> = s.to_vec();
    let mut sr = r.len();
    let mut i = 0usize;
    let mut j = 0usize;

    while i < sr {
        if !r[i].eq_ignore_ascii_case(&a[j]) {
            i -= j;
            j = 0;
            i += 1;
            continue;
        }
        j += 1;
        if j < len_a {
            i += 1;
            continue;
        }
        // The match ends at the index i.
        i -= len_a - 1;
        if len_a != len_b {
            r.splice(i..i + len_a, b.iter().copied());
        } else if len_b > 0 {
            r[i..i + len_a].copy_from_slice(b);
        }
        j = 0;
        i += len_b;
        sr = sr + len_b - len_a;
    }
    r
}

/// Applies the file name case style of the `inboundcase` directive.
pub fn makeinboundcase(s: &mut [u8], style: InbCase) {
    match style {
        InbCase::Upper => {
            s.make_ascii_uppercase();
        }
        InbCase::Lower => {
            s.make_ascii_lowercase();
        }
        InbCase::Mixed => {
            if s.is_empty() {
                return;
            }
            s[0] = s[0].to_ascii_uppercase();
            for i in 1..s.len() {
                s[i] = if s[i - 1].is_ascii_alphanumeric() {
                    s[i].to_ascii_lowercase()
                } else {
                    s[i].to_ascii_uppercase()
                };
            }
        }
        InbCase::Save => {}
    }
}

/// The file name case style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InbCase {
    /// Keep the file name as is.
    #[default]
    Save,
    /// Convert the file name to uppercase.
    Upper,
    /// Convert the file name to lowercase.
    Lower,
    /// Convert the file name to the mixed case.
    Mixed,
}

/// Converts the string to a long value.
///
/// The function reads the leading digits and reports an overflow.
pub fn safe_atol(s: Option<&str>) -> Result<i64, &'static str> {
    let Some(s) = s else {
        return Err("Invalid argument (NULL instead string)");
    };
    let t = s.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, digits) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };

    let mut value: u64 = 0;
    let mut overflow = false;
    for c in digits.chars() {
        let Some(d) = c.to_digit(10) else {
            break;
        };
        match value.checked_mul(10).and_then(|v| v.checked_add(d as u64)) {
            Some(v) => value = v,
            None => {
                overflow = true;
                break;
            }
        }
    }

    if negative {
        if overflow || value > i64::MAX as u64 + 1 {
            return Err("Out of range: number too small");
        }
        if value == i64::MAX as u64 + 1 {
            return Ok(i64::MIN);
        }
        Ok(-(value as i64))
    } else if overflow || value > i64::MAX as u64 {
        Err("Out of range: number too big")
    } else {
        Ok(value as i64)
    }
}

/// Splits the source into `argc` words.
///
/// Returns `None` when the source has fewer words. The C code writes NUL
/// bytes into the source and returns the rest of the string; the port returns
/// the words.
pub fn parse_args(argc: usize, src: &[u8]) -> Option<Vec<&[u8]>> {
    let mut args = Vec::with_capacity(argc);
    let mut i = 0usize;
    while args.len() < argc {
        while i < src.len() && is_space(src[i]) {
            i += 1;
        }
        if i >= src.len() {
            break;
        }
        let start = i;
        while i < src.len() && !is_space(src[i]) {
            i += 1;
        }
        args.push(&src[start..i]);
    }
    if args.len() < argc {
        None
    } else {
        Some(args)
    }
}

/// Returns the last directory separator of the path.
pub fn last_slash(s: &[u8]) -> Option<&[u8]> {
    let mut ret = None;
    for (i, &b) in s.iter().enumerate() {
        if b == b'/' || b == b'\\' {
            ret = Some(&s[i..]);
        }
    }
    ret
}

/// Returns the file name of the path.
pub fn extract_filename(s: &[u8]) -> &[u8] {
    let slash = s
        .iter()
        .rposition(|&b| b == b'/' || b == b'\\')
        .map(|i| i + 1)
        .unwrap_or(0);
    let colon = s
        .iter()
        .rposition(|&b| b == b':')
        .map(|i| i + 1)
        .unwrap_or(0);
    &s[slash.max(colon)..]
}

/// Returns the name of a file on the wire, the port of `netname_()` of
/// `prothlp.c`.
///
/// The C code announces a file of the mail and a file of a request with a
/// random name, and it keeps the name of every other file. The C code holds the
/// kind of a file in the queue, the port of `TFILE.type`, and the port takes
/// the kind from the name, as the scan of `q_add_dir()` of `ftnq.c` does. The
/// C code writes the week of an arcmail name in place of the first two
/// characters of the extension, the port of the `weekext` table.
pub fn netname(name: &[u8], rnd: &mut impl FnMut() -> u32) -> Vec<u8> {
    let ext = match name.iter().rposition(|c| *c == b'.') {
        Some(i) => &name[i + 1..],
        None => &name[..0],
    };
    // The C code announces a file of the mail with a random name, `type == 'm'`.
    if ext.len() == 3 && ext[1].eq_ignore_ascii_case(&b'u') && ext[2].eq_ignore_ascii_case(&b't') {
        return format!("{:08x}.pkt", rnd()).into_bytes();
    }
    // The C code announces a file of a request with a random name,
    // `type == 'r'`.
    if ext.eq_ignore_ascii_case(b"req") {
        return format!("{:08x}.req", rnd()).into_bytes();
    }
    // The C code keeps the name of every other file.
    let mut name = name.to_vec();
    if ext.len() == 3 && (b'0'..=b'6').contains(&ext[0]) && ext[1] == b'#' {
        let week = week_name(ext[0] - b'0');
        let start = name.len() - ext.len();
        name[start] = week[0];
        name[start + 1] = week[1];
    }
    strquote(&name, SQ_CNTRL | SQ_SPACE)
}

/// Returns the two letters of the week, the port of the `weekext` table of
/// `netname_()` of `prothlp.c`.
fn week_name(day: u8) -> [u8; 2] {
    match day {
        0 => *b"su",
        1 => *b"mo",
        2 => *b"tu",
        3 => *b"we",
        4 => *b"th",
        5 => *b"fr",
        _ => *b"sa",
    }
}

/// Returns true for a mail packet name.
pub fn ispkt(s: &[u8]) -> bool {
    pmatch_ncase(b"*.pkt", s)
}

/// Returns true for a request name.
pub fn isreq(s: &[u8]) -> bool {
    pmatch_ncase(b"*.req", s)
}

/// Returns true for a TIC name.
pub fn istic(s: &[u8]) -> bool {
    pmatch_ncase(b"*.?ic", s)
}

/// Returns true for an arc mail name.
pub fn isarcmail(s: &[u8]) -> bool {
    pmatch_ncase(b"*.su?", s)
        || pmatch_ncase(b"*.mo?", s)
        || pmatch_ncase(b"*.tu?", s)
        || pmatch_ncase(b"*.we?", s)
        || pmatch_ncase(b"*.th?", s)
        || pmatch_ncase(b"*.fr?", s)
        || pmatch_ncase(b"*.sa?", s)
}

/// Returns the time zone offset in minutes.
///
/// The C code computes the local time zone when the value is -1. The
/// platform layer does that part (see `doc/compatibility.md`).
pub fn tz_off(tzoff: i32) -> i64 {
    (tzoff / 60) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_and_dequote() {
        let quoted = strquote(b"a\tb", SQ_CNTRL);
        assert_eq!(quoted, b"a\\x09b");
        assert_eq!(strdequote(&quoted), b"a\tb");
    }

    #[test]
    fn quote_always_quotes_the_backslash() {
        assert_eq!(strquote(b"a\\b", 0), b"a\\x5cb");
    }

    #[test]
    fn ed_replaces() {
        assert_eq!(ed(Some(b"abc"), Some(b"b"), Some(b"X")), b"aXc");
        assert_eq!(ed(Some(b"aBc"), Some(b"b"), Some(b"X")), b"aXc");
    }

    #[test]
    fn ed_grows() {
        assert_eq!(
            ed(Some(b"a.b"), Some(b"."), Some(b"................")),
            b"a................b"
        );
    }

    #[test]
    fn atol() {
        assert_eq!(safe_atol(Some("123")), Ok(123));
        assert_eq!(safe_atol(Some("-5")), Ok(-5));
        assert!(safe_atol(None).is_err());
    }
}
