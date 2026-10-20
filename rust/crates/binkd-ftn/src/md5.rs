//! MD5 (RFC 1321) and HMAC-MD5 (RFC 2104), the port of `md5b.c`.

/// The digest length in bytes.
pub const DIGEST_LEN: usize = 16;

/// The block size in bytes.
const BLOCK_LEN: usize = 64;

const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

const K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

/// An incremental MD5 context.
#[derive(Clone)]
pub struct Md5 {
    state: [u32; 4],
    count: u64,
    buffer: [u8; BLOCK_LEN],
    buffer_len: usize,
}

impl Default for Md5 {
    fn default() -> Self {
        Self::new()
    }
}

impl Md5 {
    /// Creates a context.
    pub fn new() -> Md5 {
        Md5 {
            state: [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476],
            count: 0,
            buffer: [0; BLOCK_LEN],
            buffer_len: 0,
        }
    }

    /// Adds data to the context.
    pub fn update(&mut self, mut data: &[u8]) {
        self.count = self.count.wrapping_add(data.len() as u64);

        if self.buffer_len > 0 {
            let take = (BLOCK_LEN - self.buffer_len).min(data.len());
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&data[..take]);
            self.buffer_len += take;
            data = &data[take..];
            if self.buffer_len == BLOCK_LEN {
                let block = self.buffer;
                self.transform(&block);
                self.buffer_len = 0;
            }
        }

        while data.len() >= BLOCK_LEN {
            let mut block = [0u8; BLOCK_LEN];
            block.copy_from_slice(&data[..BLOCK_LEN]);
            self.transform(&block);
            data = &data[BLOCK_LEN..];
        }

        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffer_len = data.len();
        }
    }

    /// Returns the digest and consumes the context.
    pub fn finish(mut self) -> [u8; DIGEST_LEN] {
        let bit_len = self.count.wrapping_mul(8);
        let mut padding = vec![0x80u8];
        while (self.count + padding.len() as u64) % BLOCK_LEN as u64 != 56 {
            padding.push(0);
        }
        padding.extend_from_slice(&bit_len.to_le_bytes());
        self.update(&padding);

        let mut out = [0u8; DIGEST_LEN];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    fn transform(&mut self, block: &[u8; BLOCK_LEN]) {
        let mut m = [0u32; 16];
        for (i, word) in m.iter_mut().enumerate() {
            *word = u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }

        let [mut a, mut b, mut c, mut d] = self.state;

        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let tmp = d;
            d = c;
            c = b;
            let sum = a
                .wrapping_add(f)
                .wrapping_add(K[i])
                .wrapping_add(m[g]);
            b = b.wrapping_add(sum.rotate_left(S[i]));
            a = tmp;
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
    }
}

/// Returns the MD5 digest of the data.
pub fn md5(data: &[u8]) -> [u8; DIGEST_LEN] {
    let mut ctx = Md5::new();
    ctx.update(data);
    ctx.finish()
}

/// Returns the HMAC-MD5 digest of the data with the key (RFC 2104).
pub fn hmac_md5(key: &[u8], data: &[u8]) -> [u8; DIGEST_LEN] {
    let mut k_ipad = [0u8; BLOCK_LEN];
    let mut k_opad = [0u8; BLOCK_LEN];

    // A key longer than the block size is replaced by its digest.
    let key = if key.len() > BLOCK_LEN {
        md5(key).to_vec()
    } else {
        key.to_vec()
    };

    k_ipad[..key.len()].copy_from_slice(&key);
    k_opad[..key.len()].copy_from_slice(&key);
    for i in 0..BLOCK_LEN {
        k_ipad[i] ^= 0x36;
        k_opad[i] ^= 0x5c;
    }

    let mut inner = Md5::new();
    inner.update(&k_ipad);
    inner.update(data);
    let digest = inner.finish();

    let mut outer = Md5::new();
    outer.update(&k_opad);
    outer.update(&digest);
    outer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc1321_vectors() {
        assert_eq!(md5(b""), hex("d41d8cd98f00b204e9800998ecf8427e"));
        assert_eq!(md5(b"abc"), hex("900150983cd24fb0d6963f7d28e17f72"));
        assert_eq!(
            md5(b"The quick brown fox jumps over the lazy dog"),
            hex("9e107d9d372bb6826bd81d3542a419d6")
        );
    }

    fn hex(s: &str) -> [u8; DIGEST_LEN] {
        let mut out = [0u8; DIGEST_LEN];
        for i in 0..DIGEST_LEN {
            out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }
}
