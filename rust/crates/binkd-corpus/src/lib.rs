//! The golden corpus reader for the tests.
//!
//! The corpus is in `tests/golden/` of the repository. The C runner in
//! `tests/test_corpus.c` reads the same files, so the corpus holds the
//! two implementations together.
//!
//! A case line has these fields, separated by a tab:
//!
//! ```text
//! <func> <arg>... => <want>
//! ```
//!
//! A field escapes these characters:
//!
//! - `\\` is a backslash.
//! - `\t` is a tab.
//! - `\n` is a newline.
//! - `\r` is a carriage return.
//! - `\xHH` is the byte `HH`.
//!
//! The text `<null>` means a NULL value. Lines that start with `#` are
//! comments. Empty lines are ignored.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

/// One corpus case.
#[derive(Clone, Debug)]
pub struct Case {
    /// The path of the corpus file.
    pub file: String,
    /// The line number in the corpus file.
    pub line: usize,
    /// The crate that owns the case.
    pub crate_name: String,
    /// The function name.
    pub func: String,
    /// The decoded arguments.
    pub args: Vec<Vec<u8>>,
    /// The decoded expected value.
    pub want: Vec<u8>,
}

/// Returns the directory of the golden corpus.
pub fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/golden")
}

/// Reads every case of the corpus.
pub fn load() -> Vec<Case> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(golden_dir())
        .expect("cannot read the golden corpus")
        .map(|entry| entry.expect("cannot read the golden corpus").path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("tsv"))
        .collect();
    files.sort();

    let mut cases = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).expect("cannot read a corpus file");
        let file = path.display().to_string();
        let mut crate_name = String::new();
        for (i, line) in text.lines().enumerate() {
            let lineno = i + 1;
            if let Some(rest) = line.strip_prefix("# crate:") {
                crate_name = rest.trim().to_string();
                continue;
            }
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            let sep = fields
                .iter()
                .position(|f| *f == "=>")
                .unwrap_or_else(|| panic!("{file}:{lineno}: no => separator"));
            assert!(sep + 1 < fields.len(), "{file}:{lineno}: no expected value");
            let args = fields[1..sep]
                .iter()
                .map(|f| decode_field(f).unwrap_or_else(|| panic!("{file}:{lineno}: bad escape")))
                .collect();
            let want = decode_field(fields[sep + 1])
                .unwrap_or_else(|| panic!("{file}:{lineno}: bad escape"));
            cases.push(Case {
                file: file.clone(),
                line: lineno,
                crate_name: crate_name.clone(),
                func: fields[0].to_string(),
                args,
                want,
            });
        }
    }
    cases
}

/// Decodes a corpus field.
pub fn decode_field(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        i += 1;
        match *bytes.get(i)? {
            b'\\' => {
                out.push(b'\\');
                i += 1;
            }
            b't' => {
                out.push(b'\t');
                i += 1;
            }
            b'n' => {
                out.push(b'\n');
                i += 1;
            }
            b'r' => {
                out.push(b'\r');
                i += 1;
            }
            b'x' => {
                let hi = hex_value(*bytes.get(i + 1)?)?;
                let lo = hex_value(*bytes.get(i + 2)?)?;
                out.push(hi * 16 + lo);
                i += 3;
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Encodes a value for a message.
pub fn encode_field(s: &[u8]) -> String {
    let mut out = String::new();
    for &b in s {
        if b.is_ascii_control() || b == b' ' || b == b'\\' || b >= 0x80 {
            out.push_str(&format!("\\x{b:02x}"));
        } else {
            out.push(b as char);
        }
    }
    out
}

fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Decodes a hex string into bytes.
///
/// An odd number of digits gives `None`. The code does not use
/// `usize::is_multiple_of()`, because it needs Rust 1.87 and the workspace
/// keeps the minimum supported version lower.
pub fn hex_decode(s: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut pairs = s.chunks_exact(2);
    for pair in &mut pairs {
        out.push(hex_value(pair[0])? * 16 + hex_value(pair[1])?);
    }
    if !pairs.remainder().is_empty() {
        return None;
    }
    Some(out)
}

/// Decodes the first `len` bytes of a hex string.
pub fn hex_decode_prefix(s: &[u8], len: usize) -> Option<Vec<u8>> {
    if s.len() < len * 2 {
        return None;
    }
    hex_decode(&s[..len * 2])
}

/// Encodes bytes as a hex string.
pub fn hex_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for &b in data {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Runs the cases of one crate and returns the number of the checks.
///
/// The function panics with a report when a case fails.
pub fn check(crate_name: &str, run: impl Fn(&str, &[Vec<u8>]) -> Option<Vec<u8>>) -> usize {
    let cases = load();
    let mut checks = 0;
    let mut failures = Vec::new();
    for case in cases.iter().filter(|c| c.crate_name == crate_name) {
        let got = run(&case.func, &case.args);
        checks += 1;
        let got_text = got.as_deref().map(encode_field);
        let want_text = encode_field(&case.want);
        if got_text.as_deref() != Some(want_text.as_str()) {
            failures.push(format!(
                "{}:{}: FAIL: {}: got \"{}\", want \"{}\"",
                case.file,
                case.line,
                case.func,
                got_text.unwrap_or_else(|| "(null)".to_string()),
                want_text
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} checks failed:\n{}",
        failures.len(),
        checks,
        failures.join("\n")
    );
    assert!(checks > 0, "no cases for {crate_name}");
    checks
}
