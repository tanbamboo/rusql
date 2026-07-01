//! `mysql_native_password` authentication (MySQL 4.1+).

pub const AUTH_PLUGIN_NATIVE: &str = "mysql_native_password";

/// Compute the 20-byte native password response expected from the client.
pub fn native_password_scramble(password: &str, scramble: &[u8; 20]) -> [u8; 20] {
    let stage1 = sha1_digest(password.as_bytes());
    let stage2 = sha1_digest(&stage1);
    let mut input = [0u8; 40];
    input[..20].copy_from_slice(scramble);
    input[20..].copy_from_slice(&stage2);
    let stage3 = sha1_digest(&input);
    let mut out = [0u8; 20];
    for i in 0..20 {
        out[i] = stage1[i] ^ stage3[i];
    }
    out
}

/// Verify client auth response against configured password and server scramble.
pub fn verify_native_password(password: &str, scramble: &[u8; 20], auth_response: &[u8]) -> bool {
    if password.is_empty() {
        return auth_response.is_empty();
    }
    if auth_response.len() != 20 {
        return false;
    }
    let expected = native_password_scramble(password, scramble);
    auth_response == expected
}

fn sha1_digest(data: &[u8]) -> [u8; 20] {
    use sha1::{Digest, Sha1};
    let hash = Sha1::digest(data);
    let mut out = [0u8; 20];
    out.copy_from_slice(&hash);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_password_empty_response() {
        let scramble = [1u8; 20];
        assert!(verify_native_password("", &scramble, &[]));
        assert!(!verify_native_password("secret", &scramble, &[]));
    }

    #[test]
    fn roundtrip_scramble() {
        let scramble = [
            0x4a, 0x2b, 0x11, 0x3c, 0x5d, 0x6e, 0x7f, 0x80, 0x91, 0xa2, 0xb3, 0xc4, 0xd5, 0xe6,
            0xf7, 0x08, 0x19, 0x2a, 0x3b, 0x4c,
        ];
        let response = native_password_scramble("hunter2", &scramble);
        assert!(verify_native_password("hunter2", &scramble, &response));
        assert!(!verify_native_password("wrong", &scramble, &response));
    }
}
