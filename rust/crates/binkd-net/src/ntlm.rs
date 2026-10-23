//! The NTLM support of the proxy, the port of `ntlm/helpers.c`.
//!
//! The C code builds the messages of the NTLM handshake from the user data of the
//! `proxy` directive: `username/password/host/domain`. The port does the same, so
//! the bytes of the messages match the bytes of the C code.

use crate::base64;
use crate::des::ecb_encrypt;
use crate::md4::{digest, DIGEST_LEN};

/// The magic of the LM hash, `"KGS!@#$%"`.
const MAGIC: [u8; 8] = [0x4b, 0x47, 0x53, 0x21, 0x40, 0x23, 0x24, 0x25];

/// The offset of the `data` field of the two messages, `0x40`.
const DATA_OFFSET: usize = 0x40;

/// Returns the LM and the NT response of a password, the port of
/// `ntlm_encrypt()`.
///
/// The C code takes the index of the password and uppercases the bytes of the
/// password from it; every caller passes zero, so the port uppercases the whole
/// password.
pub fn ntlm_encrypt(password: &[u8], nonce: &[u8; 8]) -> ([u8; 24], [u8; 24]) {
    // The LM hash: the password uppercased, cut to 14 bytes, zero padded.
    let mut lm_pw = [0u8; 14];
    for (i, &byte) in password.iter().take(14).enumerate() {
        lm_pw[i] = byte.to_ascii_uppercase();
    }
    let mut lm_hpw = [0u8; 21];
    lm_hpw[..8].copy_from_slice(&ecb_encrypt(&setup_des_key(&lm_pw[..7]), &MAGIC, true));
    lm_hpw[8..16].copy_from_slice(&ecb_encrypt(&setup_des_key(&lm_pw[7..]), &MAGIC, true));

    // The NT hash: the password as two bytes per character.
    let mut nt_pw = Vec::with_capacity(password.len() * 2);
    for &byte in password {
        nt_pw.push(byte);
        nt_pw.push(0);
    }
    let mut nt_hpw = [0u8; 21];
    nt_hpw[..DIGEST_LEN].copy_from_slice(&digest(&nt_pw));

    // The responses of the two hashes.
    (calc_resp(&lm_hpw, nonce), calc_resp(&nt_hpw, nonce))
}

/// Returns the 24 byte response of a 21 byte hash, the port of `calc_resp()`.
fn calc_resp(hash: &[u8; 21], nonce: &[u8; 8]) -> [u8; 24] {
    let mut out = [0u8; 24];
    for i in 0..3 {
        let key = setup_des_key(&hash[i * 7..i * 7 + 7]);
        out[i * 8..i * 8 + 8].copy_from_slice(&ecb_encrypt(&key, nonce, true));
    }
    out
}

/// Returns the 8 byte DES key of 7 bytes, the port of `setup_des_key()`.
fn setup_des_key(key_56: &[u8]) -> [u8; 8] {
    [
        key_56[0],
        (key_56[0] << 7) | (key_56[1] >> 1),
        (key_56[1] << 6) | (key_56[2] >> 2),
        (key_56[2] << 5) | (key_56[3] >> 3),
        (key_56[3] << 4) | (key_56[4] >> 4),
        (key_56[4] << 3) | (key_56[5] >> 5),
        (key_56[5] << 2) | (key_56[6] >> 6),
        (key_56[6] << 1),
    ]
}

/// Returns the `NTLMSSP` header of a message, the port of the `protocol` field.
fn protocol() -> [u8; 8] {
    *b"NTLMSSP\0"
}

/// Returns the base64 text of the Type 1 message, the port of `getNTLM1()`.
///
/// The user data is `username/password/host/domain`. The C code reads the host
/// and the domain, and it ignores the user and the password. The error value of the
/// C code is the number of the missing part: 1 is the domain and 2 is the host.
/// The C code answers -1 for a result buffer that is too small; the port holds no
/// fixed buffer, so the error cannot arise.
pub fn get_ntlm1(udata: &str) -> Result<String, u32> {
    let Some((prefix, domain)) = udata.rsplit_once('/') else {
        return Err(1);
    };
    let Some((_, host)) = prefix.rsplit_once('/') else {
        return Err(2);
    };
    let host = host.to_uppercase();
    let domain = domain.to_uppercase();

    let mut message = Vec::with_capacity(DATA_OFFSET + host.len() + domain.len());
    message.extend_from_slice(&protocol());
    message.push(1);
    message.extend_from_slice(&[0, 0, 0]);
    message.extend_from_slice(&short(0xb203));
    message.extend_from_slice(&[0, 0]);
    message.extend_from_slice(&short(domain.len() as u16));
    message.extend_from_slice(&short(domain.len() as u16));
    message.extend_from_slice(&short((DATA_OFFSET + host.len()) as u16));
    message.extend_from_slice(&[0, 0]);
    message.extend_from_slice(&short(host.len() as u16));
    message.extend_from_slice(&short(host.len() as u16));
    message.extend_from_slice(&short(DATA_OFFSET as u16));
    message.extend_from_slice(&[0, 0]);
    message.extend_from_slice(host.as_bytes());
    message.extend_from_slice(domain.as_bytes());
    Ok(base64::encode(&message))
}

/// Returns the base64 text of the Type 3 message, the port of `getNTLM2()`.
///
/// The user data is `username/password/host/domain`, and the request is the
/// base64 token of the `WWW-Authenticate` or the `Proxy-Authenticate` header of
/// the answer. The error value of the C code is the number of the missing part: 1
/// is the domain, 2 is the host, and 3 is the password. The C code answers -1
/// for a result buffer that is too small; the port holds no fixed buffer, so the
/// error cannot arise.
pub fn get_ntlm2(udata: &str, request: &str) -> Result<String, u32> {
    let Some((prefix, domain)) = udata.rsplit_once('/') else {
        return Err(1);
    };
    let Some((prefix, host)) = prefix.rsplit_once('/') else {
        return Err(2);
    };
    let Some((user, password)) = prefix.rsplit_once('/') else {
        return Err(3);
    };
    let domain = domain.to_uppercase();
    let host = host.to_uppercase();

    // The nonce of the challenge, the bytes 24 to 32 of the decoded token.
    let challenge = base64::decode(request.as_bytes());
    let mut nonce = [0u8; 8];
    if let Some(bytes) = challenge.get(24..32) {
        nonce.copy_from_slice(bytes);
    }

    let mut message = Vec::with_capacity(DATA_OFFSET);
    message.extend_from_slice(&protocol());
    message.push(3);
    message.extend_from_slice(&[0, 0, 0]);
    message.extend_from_slice(&short(0x18));
    message.extend_from_slice(&short(0x18));
    message.extend_from_slice(&short(
        (DATA_OFFSET + data_len(&domain, user, &host)) as u16,
    ));
    message.extend_from_slice(&[0, 0]);
    message.extend_from_slice(&short(0x18));
    message.extend_from_slice(&short(0x18));
    message.extend_from_slice(&short(
        (DATA_OFFSET + 0x18 + data_len(&domain, user, &host)) as u16,
    ));
    message.extend_from_slice(&[0, 0]);
    message.extend_from_slice(&short((domain.len() * 2) as u16));
    message.extend_from_slice(&short((domain.len() * 2) as u16));
    message.extend_from_slice(&short(DATA_OFFSET as u16));
    message.extend_from_slice(&[0, 0]);
    message.extend_from_slice(&short((user.len() * 2) as u16));
    message.extend_from_slice(&short((user.len() * 2) as u16));
    message.extend_from_slice(&short((DATA_OFFSET + domain.len() * 2) as u16));
    message.extend_from_slice(&[0, 0]);
    message.extend_from_slice(&short((host.len() * 2) as u16));
    message.extend_from_slice(&short((host.len() * 2) as u16));
    message.extend_from_slice(&short(
        (DATA_OFFSET + domain.len() * 2 + user.len() * 2) as u16,
    ));
    message.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    let length = DATA_OFFSET + data_len(&domain, user, &host) + 48;
    message.extend_from_slice(&short(length as u16));
    message.extend_from_slice(&[0, 0]);
    message.extend_from_slice(&short(0x8201));
    message.extend_from_slice(&[0, 0]);

    // The data: the domain, the user, the host, and the two responses. The C
    // code writes the low byte of every character and leaves the high byte zero.
    for &byte in domain.as_bytes() {
        message.push(byte.to_ascii_uppercase());
        message.push(0);
    }
    for &byte in user.as_bytes() {
        message.push(byte);
        message.push(0);
    }
    for &byte in host.as_bytes() {
        message.push(byte);
        message.push(0);
    }
    let (lm_resp, nt_resp) = ntlm_encrypt(password.as_bytes(), &nonce);
    message.extend_from_slice(&lm_resp);
    message.extend_from_slice(&nt_resp);
    Ok(base64::encode(&message))
}

/// Returns the length of the data of the Type 3 message.
fn data_len(domain: &str, user: &str, host: &str) -> usize {
    (domain.len() + user.len() + host.len()) * 2
}

/// Returns the little-endian bytes of a short value, the port of `mkls()`.
fn short(value: u16) -> [u8; 2] {
    value.to_le_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example of the NTLM documentation: the password `SecREt01` and the
    /// challenge `0123456789abcdef` give the responses of the tables of
    /// MS-NLMP.
    #[test]
    fn the_responses_of_the_documentation() {
        let nonce = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
        let (lm, nt) = ntlm_encrypt(b"SecREt01", &nonce);
        assert_eq!(
            lm,
            [
                0xc3, 0x37, 0xcd, 0x5c, 0xbd, 0x44, 0xfc, 0x97, 0x82, 0xa6, 0x67, 0xaf, 0x6d, 0x42,
                0x7c, 0x6d, 0xe6, 0x7c, 0x20, 0xc2, 0xd3, 0xe7, 0x7c, 0x56
            ]
        );
        assert_eq!(
            nt,
            [
                0x25, 0xa9, 0x8c, 0x1c, 0x31, 0xe8, 0x18, 0x47, 0x46, 0x6b, 0x29, 0xb2, 0xdf, 0x46,
                0x80, 0xf3, 0x99, 0x58, 0xfb, 0x8c, 0x21, 0x3a, 0x9c, 0xc6
            ]
        );
    }

    #[test]
    fn the_type_one_message() {
        let message = get_ntlm1("User/SecREt01/HOST/Domain").unwrap();
        let bytes = base64::decode(message.as_bytes());
        assert_eq!(&bytes[..8], b"NTLMSSP\0");
        assert_eq!(bytes[8], 1);
        assert_eq!(&bytes[12..14], &[0x03, 0xb2]);
        // The domain and the host, both uppercased, after the 0x20 byte header.
        assert_eq!(&bytes[0x20..], b"HOSTDOMAIN");
    }

    #[test]
    fn the_errors_of_the_user_data() {
        assert_eq!(get_ntlm1("User/SecREt01"), Err(2));
        assert_eq!(get_ntlm1("User"), Err(1));
        // The C code reads the domain, then the host, then the password, and
        // it answers the number of the part that is missing.
        assert_eq!(get_ntlm2("User", "AAAA"), Err(1));
        assert_eq!(get_ntlm2("User/SecREt01", "AAAA"), Err(2));
        assert_eq!(get_ntlm2("User/SecREt01/HOST", "AAAA"), Err(3));
    }

    #[test]
    fn the_type_three_message() {
        let message = get_ntlm2("User/SecREt01/HOST/Domain", "AAAA").unwrap();
        let bytes = base64::decode(message.as_bytes());
        assert_eq!(&bytes[..8], b"NTLMSSP\0");
        assert_eq!(bytes[8], 3);
        // The header holds the offsets of the responses and of the data.
        let domain = bytes[0x40..0x40 + 2 * 6].to_vec();
        assert_eq!(domain[0], b'D');
        assert_eq!(domain[1], 0);
    }
}
