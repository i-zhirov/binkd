//! CRAM-MD5 challenge handling, the port of `md5b.c`.
//!
//! The wire form of a CRAM-MD5 value is `CRAM-MD5-<hex digest>`.

use crate::md5::{hmac_md5, DIGEST_LEN};

/// The challenge length in bytes.
pub const CHALLENGE_LEN: usize = 16;

/// The prefix of the CRAM-MD5 value.
pub const PREFIX: &str = "CRAM-MD5-";

/// Returns the digest string of a challenge and a password.
///
/// The first byte of the challenge is the challenge length.
pub fn build_digest(password: &[u8], challenge: &[u8]) -> Option<String> {
    let len = *challenge.first()? as usize;
    let data = challenge.get(1..1 + len)?;
    let digest = hmac_md5(password, data);
    Some(to_string(&digest))
}

/// Returns the `CRAM-MD5-` string of the digest bytes.
pub fn to_string(digest: &[u8]) -> String {
    let mut out = String::with_capacity(PREFIX.len() + digest.len() * 2);
    out.push_str(PREFIX);
    for &b in digest {
        out.push(hex_digit(b >> 4));
        out.push(hex_digit(b & 0x0f));
    }
    out
}

fn hex_digit(v: u8) -> char {
    if v > 9 {
        (b'a' + v - 10) as char
    } else {
        (b'0' + v) as char
    }
}

/// Parses a `CRAM-MD5-<hex digest>` value.
///
/// Returns the digest bytes. The first byte of the result is the
/// digest length. The C code writes one byte past the buffer for an odd
/// number of hex digits. The port takes the full pairs only (see
/// `doc/compatibility.md`).
pub fn parse(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    let at = find(bytes, b"CRAM")?;
    let sp = find(&bytes[at..], b"MD5")? + at;
    let mut i = sp + 3;
    while i < bytes.len() {
        if bytes[i] == b'-' {
            i += 1;
            break;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let mut digits = Vec::new();
    while i < bytes.len() && digits.len() < 128 && bytes[i].is_ascii_hexdigit() {
        digits.push(bytes[i]);
        i += 1;
    }
    let len = digits.len() / 2;
    let mut out = Vec::with_capacity(len + 1);
    out.push(len as u8);
    for pair in digits.chunks(2) {
        if pair.len() < 2 {
            break;
        }
        let hi = hex_value(pair[0])?;
        let lo = hex_value(pair[1])?;
        out.push(hi * 16 + lo);
    }
    Some(out)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Returns a new challenge for the CRAM-MD5 handshake.
///
/// The C code mixes the time, the process id, a random value, and the
/// peer name. The values differ between the runs, so only the length is
/// a stable contract.
pub fn generate_challenge(peer_name: &[u8]) -> Vec<u8> {
    let mut seed = Vec::with_capacity(64);
    seed.extend_from_slice(&now_secs().to_le_bytes());
    seed.extend_from_slice(&std::process::id().to_le_bytes());
    seed.extend_from_slice(&counter().to_le_bytes());
    seed.extend_from_slice(peer_name);
    let digest = hmac_md5(&seed, b"binkd");

    let mut out = Vec::with_capacity(CHALLENGE_LEN + 1);
    out.push(CHALLENGE_LEN as u8);
    for i in 0..CHALLENGE_LEN {
        out.push(digest[i % DIGEST_LEN]);
    }
    out
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn counter() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    COUNTER.fetch_add(0x9e37_79b9, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_string_value() {
        let digest: Vec<u8> = (0..16).collect();
        assert_eq!(
            to_string(&digest),
            "CRAM-MD5-000102030405060708090a0b0c0d0e0f"
        );
    }

    #[test]
    fn parse_value() {
        let parsed = parse("CRAM-MD5-000102030405060708090a0b0c0d0e0f").unwrap();
        assert_eq!(parsed[0], 16);
        assert_eq!(parsed[1], 0x00);
        assert_eq!(parsed[16], 0x0f);
    }

    #[test]
    fn parse_rejects() {
        assert!(parse("BOGUS").is_none());
        assert!(parse("CRAM-MD5").is_none());
        assert!(parse("CRAM-MD5-").is_none());
    }

    #[test]
    fn challenge_length() {
        let ch = generate_challenge(b"peer");
        assert_eq!(ch.len(), CHALLENGE_LEN + 1);
        assert_eq!(ch[0] as usize, CHALLENGE_LEN);
    }
}
