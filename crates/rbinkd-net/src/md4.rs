//! The MD4 of the NTLM support, the port of `ntlm/md4_dgst.c` and of
//! `ntlm/md32_com.h`, the vendored OpenSSL code.

/// The length of a digest in bytes.
pub const DIGEST_LEN: usize = 16;

/// The length of a block in bytes.
const BLOCK_LEN: usize = 64;

/// The state of a digest in progress.
#[derive(Clone, Debug)]
pub struct Md4 {
    /// The four words of the state.
    state: [u32; 4],
    /// The number of the bytes that arrived.
    len: u64,
    /// The bytes of the block that waits for its end.
    block: Vec<u8>,
}

impl Default for Md4 {
    fn default() -> Md4 {
        Md4::new()
    }
}

impl Md4 {
    /// Starts a digest, the port of `MD4_Init()`.
    pub fn new() -> Md4 {
        Md4 {
            state: [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476],
            len: 0,
            block: Vec::with_capacity(BLOCK_LEN),
        }
    }

    /// Adds bytes to the digest, the port of `MD4_Update()`.
    pub fn update(&mut self, data: &[u8]) {
        self.len += data.len() as u64;
        let mut data = data;
        // The C code holds the tail of the last block in the context.
        if !self.block.is_empty() {
            let want = BLOCK_LEN - self.block.len();
            let take = want.min(data.len());
            self.block.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.block.len() == BLOCK_LEN {
                let block = self.block.clone();
                self.transform(&block);
                self.block.clear();
            }
        }
        while data.len() >= BLOCK_LEN {
            self.transform(&data[..BLOCK_LEN]);
            data = &data[BLOCK_LEN..];
        }
        self.block.extend_from_slice(data);
    }

    /// Ends the digest, the port of `MD4_Final()`.
    pub fn finish(mut self) -> [u8; DIGEST_LEN] {
        let bits = self.len * 8;
        // The padding: 0x80, then zeros, then the length in bits, little-endian.
        let mut tail = vec![0x80u8];
        while (self.len + tail.len() as u64) % BLOCK_LEN as u64 != BLOCK_LEN as u64 - 8 {
            tail.push(0);
        }
        tail.extend_from_slice(&bits.to_le_bytes());
        self.update(&tail);
        let mut out = [0u8; DIGEST_LEN];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    /// Applies the block function, the port of `md4_block_data_order()`.
    fn transform(&mut self, block: &[u8]) {
        let mut x = [0u32; 16];
        for (i, word) in x.iter_mut().enumerate() {
            *word = u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        let mut state = self.state;

        // The first round: `F()` with the constant zero.
        for i in 0..16 {
            round(&mut state, x[i], 0, [3, 7, 11, 19][i % 4], f);
        }
        // The second round: `G()` with the constant 0x5a827999.
        for i in 0..16 {
            let k = [0, 4, 8, 12, 1, 5, 9, 13, 2, 6, 10, 14, 3, 7, 11, 15][i];
            round(&mut state, x[k], 0x5a82_7999, [3, 5, 9, 13][i % 4], g);
        }
        // The third round: `H()` with the constant 0x6ed9eba1.
        for i in 0..16 {
            let k = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15][i];
            round(&mut state, x[k], 0x6ed9_eba1, [3, 9, 11, 15][i % 4], h);
        }

        for (word, add) in self.state.iter_mut().zip(state) {
            *word = word.wrapping_add(add);
        }
    }
}

/// One step of a round, the port of the `ROUND()` macros of `md32_com.h`.
///
/// The state holds the four words, and the step turns them.
fn round(state: &mut [u32; 4], x: u32, constant: u32, s: u32, f: fn(u32, u32, u32) -> u32) {
    let [a, b, c, d] = *state;
    let a = a
        .wrapping_add(f(b, c, d))
        .wrapping_add(x)
        .wrapping_add(constant)
        .rotate_left(s);
    *state = [d, a, b, c];
}

/// `F()` of `md32_com.h`.
fn f(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | (!x & z)
}

/// `G()` of `md32_com.h`.
fn g(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | (x & z) | (y & z)
}

/// `H()` of `md32_com.h`.
fn h(x: u32, y: u32, z: u32) -> u32 {
    x ^ y ^ z
}

/// Returns the digest of the bytes, the port of `MD4()`.
pub fn digest(data: &[u8]) -> [u8; DIGEST_LEN] {
    let mut md4 = Md4::new();
    md4.update(data);
    md4.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vectors_of_rfc_1320() {
        assert_eq!(
            digest(b""),
            [
                0x31, 0xd6, 0xcf, 0xe0, 0xd1, 0x6a, 0xe9, 0x31, 0xb7, 0x3c, 0x59, 0xd7, 0xe0, 0xc0,
                0x89, 0xc0
            ]
        );
        assert_eq!(
            digest(b"abc"),
            [
                0xa4, 0x48, 0x01, 0x7a, 0xaf, 0x21, 0xd8, 0x52, 0x5f, 0xc1, 0x0a, 0xe8, 0x7a, 0xa6,
                0x72, 0x9d
            ]
        );
        assert_eq!(
            digest(b"message digest"),
            [
                0xd9, 0x13, 0x0a, 0x81, 0x64, 0x54, 0x9f, 0xe8, 0x18, 0x87, 0x48, 0x06, 0xe1, 0xc7,
                0x01, 0x4b
            ]
        );
        assert_eq!(
            digest(b"abcdefghijklmnopqrstuvwxyz"),
            [
                0xd7, 0x9e, 0x1c, 0x30, 0x8a, 0xa5, 0xbb, 0xcd, 0xee, 0xa8, 0xed, 0x63, 0xdf, 0x41,
                0x2d, 0xa9
            ]
        );
    }

    #[test]
    fn the_update_holds_the_blocks() {
        // The bytes arrive in parts of every length.
        let data = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        for split in 0..=data.len() {
            let mut md4 = Md4::new();
            md4.update(&data[..split]);
            md4.update(&data[split..]);
            assert_eq!(md4.finish(), digest(data));
        }
    }
}
