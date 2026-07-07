//! `caching_sha2_password` fast authentication (MySQL 8.0 default).

pub const AUTH_PLUGIN_CACHING_SHA2: &str = "caching_sha2_password";

/// Compute the 32-byte fast-auth response (go-sql-driver / MySQL compatible).
pub fn caching_sha2_fast_scramble(password: &str, salt: &[u8; 20]) -> [u8; 32] {
    let hash_stage1 = sha256_digest(password.as_bytes());
    let hash_stage2 = sha256_digest(&hash_stage1);
    let mut message = Vec::with_capacity(52);
    message.extend_from_slice(salt);
    message.extend_from_slice(&hash_stage2);
    let scramble = sha256_digest(&message);
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = hash_stage1[i] ^ scramble[i];
    }
    out
}

/// True when the client sent no password (empty slice or all-zero bytes, as libmysql sends `[0x00]`).
pub fn is_empty_password_auth(auth_response: &[u8]) -> bool {
    auth_response.is_empty() || auth_response.iter().all(|&b| b == 0)
}

/// Verify fast-auth path (rusql accepts fast auth directly; no server-side cache).
pub fn verify_caching_sha2_fast(password: &str, salt: &[u8; 20], auth_response: &[u8]) -> bool {
    if password.is_empty() {
        return is_empty_password_auth(auth_response);
    }
    if auth_response.len() != 32 {
        return false;
    }
    let expected = caching_sha2_fast_scramble(password, salt);
    auth_response == expected
}

fn sha256_digest(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&hash);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_password_null_byte_auth() {
        let salt = [2u8; 20];
        assert!(verify_caching_sha2_fast("", &salt, &[]));
        assert!(verify_caching_sha2_fast("", &salt, &[0x00]));
        assert!(verify_caching_sha2_fast("", &salt, &[0x00, 0x00]));
    }

    #[test]
    fn roundtrip() {
        let salt = [
            0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7,
            0xf8, 0x09, 0x1a, 0x2b, 0x3c, 0x4d,
        ];
        let response = caching_sha2_fast_scramble("mysql", &salt);
        assert!(verify_caching_sha2_fast("mysql", &salt, &response));
        assert!(!verify_caching_sha2_fast("wrong", &salt, &response));
    }
}
