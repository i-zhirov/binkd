//! ZipCrypto (Schlafly) stream cipher, the port of `crypt.c`.
//!
//! The C code keeps the keys in `unsigned long` values. The Rust code
//! uses wrapping `u32` arithmetic. The two give the same key stream,
//! because the code uses only the low byte of the key shift. The corpus
//! case `crypt_decrypt` checks the result against an independent
//! 32-bit ZipCrypto reference.

/// The CRC-32 table of the cipher.
pub const CRC_TABLE: [u32; 256] = build_crc_table();

const fn build_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xedb8_8320
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

fn crc32(c: u32, b: u8) -> u32 {
    CRC_TABLE[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8)
}

/// Updates the keys with the next byte and returns the byte.
pub fn update_keys(keys: &mut [u32; 3], c: u8) -> u8 {
    keys[0] = crc32(keys[0], c);
    keys[1] = keys[1].wrapping_add(keys[0] & 0xff);
    keys[1] = keys[1].wrapping_mul(134_775_813).wrapping_add(1);
    let keyshift = (keys[1] >> 24) as u8;
    keys[2] = crc32(keys[2], keyshift);
    c
}

/// Sets up the keys for the password.
pub fn init_keys(password: &[u8]) -> [u32; 3] {
    let mut keys = [0x1234_5678u32, 0x2345_6789u32, 0x3456_7890u32];
    for &b in password {
        update_keys(&mut keys, b);
    }
    keys
}

/// Returns the next key stream byte.
pub fn decrypt_byte(keys: &[u32; 3]) -> u8 {
    let temp = (keys[2] & 0xffff) | 2;
    (temp.wrapping_mul(temp ^ 1) >> 8) as u8
}

/// Decrypts the buffer in place.
pub fn decrypt_buf(buf: &mut [u8], keys: &mut [u32; 3]) {
    for b in buf.iter_mut() {
        *b ^= decrypt_byte(keys);
        update_keys(keys, *b);
    }
}

/// Encrypts the buffer in place.
pub fn encrypt_buf(buf: &mut [u8], keys: &mut [u32; 3]) {
    for b in buf.iter_mut() {
        let t = decrypt_byte(keys);
        update_keys(keys, *b);
        *b ^= t;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert_eq!(CRC_TABLE[0], 0x0000_0000);
        assert_eq!(CRC_TABLE[1], 0x7707_3096);
        assert_eq!(CRC_TABLE[128], 0xedb8_8320);
        assert_eq!(CRC_TABLE[255], 0x2d02_ef8d);
    }

    #[test]
    fn keys() {
        let keys = init_keys(b"password");
        assert_eq!(keys[0], 0xea9b_4e4d);
        assert_eq!(keys[1], 0xba78_9085);
        assert_eq!(keys[2], 0x5ff8_707d);

        let keys = init_keys(b"");
        assert_eq!(keys, [0x1234_5678, 0x2345_6789, 0x3456_7890]);
    }

    #[test]
    fn round_trip() {
        let data: Vec<u8> = (0..64).map(|i| (i * 7) as u8).collect();
        let mut buf = data.clone();
        let mut keys = init_keys(b"secret");
        encrypt_buf(&mut buf, &mut keys);
        let mut keys = init_keys(b"secret");
        decrypt_buf(&mut buf, &mut keys);
        assert_eq!(buf, data);
    }

    #[test]
    fn known_answer() {
        // The value comes from an independent 32-bit ZipCrypto reference.
        let want: [u8; 16] = [
            0xee, 0x79, 0x68, 0x26, 0xf3, 0x52, 0x69, 0xf0, 0x32, 0xb4, 0x51, 0x9f, 0x97, 0x88,
            0x9b, 0xc9,
        ];
        let mut buf: Vec<u8> = (0..16).collect();
        let mut keys = init_keys(b"password");
        decrypt_buf(&mut buf, &mut keys);
        assert_eq!(buf, want);
    }
}
