//! The DES of the NTLM support.
//!
//! The port of `ntlm/des_enc.c`, `ntlm/set_key.c`, and `ntlm/ecb_enc.c`, the
//! vendored OpenSSL code. The tables of `des_tables.rs` are the ones of that code,
//! and the port keeps the operations, so the cipher text is the same. The C code
//! holds `des_check_key` at zero, so `des_set_key()` always calls
//! `des_set_key_unchecked()`: the port does the same.

use crate::des_tables::{ODD_PARITY, SKB, SP_TRANS};

/// The number of the rounds, `ITERATIONS`.
const ITERATIONS: usize = 16;

/// The schedule of a key: the 16 round keys with two words each.
pub type Schedule = [u32; 32];

/// Applies the permutation of `PERM_OP` of `des_locl.h`.
fn perm_op(a: &mut u32, b: &mut u32, n: u32, m: u32) {
    let t = ((*a >> n) ^ *b) & m;
    *b ^= t;
    *a ^= t << n;
}

/// Applies the permutation of `HPERM_OP` of `des_locl.h`.
fn hperm_op(a: &mut u32, n: i32, m: u32) {
    let shift = (16 - n) as u32;
    let t = ((*a << shift) ^ *a) & m;
    *a ^= t ^ (t >> shift);
}

/// Returns the key schedule of a key, the port of `des_set_key_unchecked()`.
pub fn set_key(key: &[u8; 8]) -> Schedule {
    const SHIFTS2: [bool; 16] = [
        false, false, true, true, true, true, true, true, false, true, true, true, true, true,
        true, false,
    ];
    let mut c = u32::from_le_bytes([key[0], key[1], key[2], key[3]]);
    let mut d = u32::from_le_bytes([key[4], key[5], key[6], key[7]]);

    // The PC1 permutation of the key.
    perm_op(&mut d, &mut c, 4, 0x0f0f_0f0f);
    hperm_op(&mut c, -2, 0xcccc_0000);
    hperm_op(&mut d, -2, 0xcccc_0000);
    perm_op(&mut d, &mut c, 1, 0x5555_5555);
    perm_op(&mut c, &mut d, 8, 0x00ff_00ff);
    perm_op(&mut d, &mut c, 1, 0x5555_5555);
    d = ((d & 0x0000_00ff) << 16)
        | (d & 0x0000_ff00)
        | ((d & 0x00ff_0000) >> 16)
        | ((c & 0xf000_0000) >> 4);
    c &= 0x0fff_ffff;

    let mut schedule = [0u32; 32];
    for i in 0..ITERATIONS {
        if SHIFTS2[i] {
            c = (c >> 2) | (c << 26);
            d = (d >> 2) | (d << 26);
        } else {
            c = (c >> 1) | (c << 27);
            d = (d >> 1) | (d << 27);
        }
        c &= 0x0fff_ffff;
        d &= 0x0fff_ffff;
        // The PC2 permutation of the round key.
        let s = SKB[0][(c & 0x3f) as usize]
            | SKB[1][((c >> 6) & 0x03 | (c >> 7) & 0x3c) as usize]
            | SKB[2][((c >> 13) & 0x0f | (c >> 14) & 0x30) as usize]
            | SKB[3][(((c >> 20) & 0x01) | ((c >> 21) & 0x06) | ((c >> 22) & 0x38)) as usize];
        let t = SKB[4][(d & 0x3f) as usize]
            | SKB[5][((d >> 7) & 0x03 | (d >> 8) & 0x3c) as usize]
            | SKB[6][((d >> 15) & 0x3f) as usize]
            | SKB[7][(((d >> 21) & 0x0f) | ((d >> 22) & 0x30)) as usize];

        // The table contained 0213 4657.
        let t2 = (t << 16) | (s & 0x0000_ffff);
        schedule[i * 2] = t2.rotate_right(30);
        let t2 = (s >> 16) | (t & 0xffff_0000);
        schedule[i * 2 + 1] = t2.rotate_right(26);
    }
    schedule
}

/// Returns the odd parity of a byte, the port of `des_set_odd_parity()`.
pub fn set_odd_parity(key: &mut [u8; 8]) {
    for byte in key.iter_mut() {
        *byte = ODD_PARITY[*byte as usize];
    }
}

/// The round of `D_ENCRYPT` of `des_locl.h`.
fn round(l: &mut u32, r: u32, s: &[u32]) {
    let u = r ^ s[0];
    let t = (r ^ s[1]).rotate_right(4);
    *l ^= SP_TRANS[0][((u >> 2) & 0x3f) as usize]
        | SP_TRANS[2][((u >> 10) & 0x3f) as usize]
        | SP_TRANS[4][((u >> 18) & 0x3f) as usize]
        | SP_TRANS[6][((u >> 26) & 0x3f) as usize]
        | SP_TRANS[1][((t >> 2) & 0x3f) as usize]
        | SP_TRANS[3][((t >> 10) & 0x3f) as usize]
        | SP_TRANS[5][((t >> 18) & 0x3f) as usize]
        | SP_TRANS[7][((t >> 26) & 0x3f) as usize];
}

/// Encrypts or decrypts one block of two words, the port of `des_encrypt()`.
///
/// The block holds the two words in the order of the C code: `data[0]` is the
/// right half and `data[1]` the left half.
pub fn encrypt(data: &mut [u32; 2], ks: &Schedule, enc: bool) {
    let mut r = data[0];
    let mut l = data[1];

    // The initial permutation `IP(r, l)`.
    perm_op(&mut l, &mut r, 4, 0x0f0f_0f0f);
    perm_op(&mut r, &mut l, 16, 0x0000_ffff);
    perm_op(&mut l, &mut r, 2, 0x3333_3333);
    perm_op(&mut r, &mut l, 8, 0x00ff_00ff);
    perm_op(&mut l, &mut r, 1, 0x5555_5555);

    r = r.rotate_right(29);
    l = l.rotate_right(29);

    if enc {
        for i in (0..32).step_by(8) {
            round(&mut l, r, &ks[i..]);
            round(&mut r, l, &ks[i + 2..]);
            round(&mut l, r, &ks[i + 4..]);
            round(&mut r, l, &ks[i + 6..]);
        }
    } else {
        for i in (0..32).step_by(8) {
            round(&mut l, r, &ks[30 - i..]);
            round(&mut r, l, &ks[28 - i..]);
            round(&mut l, r, &ks[26 - i..]);
            round(&mut r, l, &ks[24 - i..]);
        }
    }

    l = l.rotate_right(3);
    r = r.rotate_right(3);

    // The final permutation `FP(r, l)`.
    perm_op(&mut r, &mut l, 1, 0x5555_5555);
    perm_op(&mut l, &mut r, 8, 0x00ff_00ff);
    perm_op(&mut r, &mut l, 2, 0x3333_3333);
    perm_op(&mut l, &mut r, 16, 0x0000_ffff);
    perm_op(&mut r, &mut l, 4, 0x0f0f_0f0f);

    data[0] = l;
    data[1] = r;
}

/// Encrypts or decrypts one 8 byte block with a key, the port of
/// `des_ecb_encrypt()`.
///
/// The C code computes the key schedule with `setup_des_key()`, which sets the odd
/// parity of the key first, and the port does the same.
pub fn ecb_encrypt(key: &[u8; 8], block: &[u8; 8], enc: bool) -> [u8; 8] {
    let mut key = *key;
    set_odd_parity(&mut key);
    let ks = set_key(&key);
    let mut data = [
        u32::from_le_bytes([block[0], block[1], block[2], block[3]]),
        u32::from_le_bytes([block[4], block[5], block[6], block[7]]),
    ];
    encrypt(&mut data, &ks, enc);
    let mut out = [0u8; 8];
    out[..4].copy_from_slice(&data[0].to_le_bytes());
    out[4..].copy_from_slice(&data[1].to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test vector of the FIPS 46 example: the key `133457799BBCDFF1` and the
    /// plain text `0123456789ABCDEF` give the cipher text `85E813540F0AB405`.
    #[test]
    fn fips_vector() {
        let key = [0x13, 0x34, 0x57, 0x79, 0x9b, 0xbc, 0xdf, 0xf1];
        let plain = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
        let cipher = [0x85, 0xe8, 0x13, 0x54, 0x0f, 0x0a, 0xb4, 0x05];
        assert_eq!(ecb_encrypt(&key, &plain, true), cipher);
        assert_eq!(ecb_encrypt(&key, &cipher, false), plain);
    }

    /// The odd parity of the key: the C code sets it before the schedule.
    #[test]
    fn odd_parity() {
        let mut key = [0u8; 8];
        set_odd_parity(&mut key);
        assert_eq!(key, [1, 1, 1, 1, 1, 1, 1, 1]);
        let mut key = [0xffu8; 8];
        set_odd_parity(&mut key);
        assert_eq!(key, [0xfe; 8]);
    }
}
