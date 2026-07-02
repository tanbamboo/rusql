//! RSA full-auth path for `caching_sha2_password` (non-TLS connections).

use rsa::pkcs8::{DecodePublicKey, EncodePublicKey, LineEnding};
use rsa::{Oaep, RsaPrivateKey, RsaPublicKey};
use sha1::Sha1;

pub const AUTH_MORE_DATA_TAG: u8 = 0x01;
pub const CACHING_SHA2_FAST_AUTH_OK: u8 = 0x03;
pub const CACHING_SHA2_FULL_AUTH: u8 = 0x04;
pub const CACHING_SHA2_PUBLIC_KEY_REQUEST: u8 = 0x02;

/// Server RSA key pair for caching_sha2 full authentication.
#[derive(Clone, Debug)]
pub struct CachingSha2RsaKeys {
    private_key: RsaPrivateKey,
}

impl CachingSha2RsaKeys {
    pub fn generate() -> Result<Self, rsa::errors::Error> {
        let mut rng = rand::thread_rng();
        Ok(Self {
            private_key: RsaPrivateKey::new(&mut rng, 2048)?,
        })
    }

    pub fn public_key_pem(&self) -> Vec<u8> {
        self.private_key
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("encode public key")
            .into_bytes()
    }

    pub fn decrypt_password(
        &self,
        ciphertext: &[u8],
        scramble: &[u8; 20],
    ) -> Result<String, rsa::errors::Error> {
        let padding = Oaep::new::<Sha1>();
        let plain = self.private_key.decrypt(padding, ciphertext)?;
        Ok(descramble_password(&plain, scramble))
    }
}

pub fn auth_more_data_fast_auth_ok() -> Vec<u8> {
    vec![AUTH_MORE_DATA_TAG, CACHING_SHA2_FAST_AUTH_OK]
}

pub fn auth_more_data_full_auth_required() -> Vec<u8> {
    vec![AUTH_MORE_DATA_TAG, CACHING_SHA2_FULL_AUTH]
}

pub fn auth_more_data_public_key(pem: &[u8]) -> Vec<u8> {
    let mut p = vec![AUTH_MORE_DATA_TAG];
    p.extend_from_slice(pem);
    p
}

pub fn is_public_key_request(payload: &[u8]) -> bool {
    payload == [CACHING_SHA2_PUBLIC_KEY_REQUEST]
}

pub fn scramble_password(password: &str, scramble: &[u8; 20]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(password.len() + 1);
    buf.extend_from_slice(password.as_bytes());
    buf.push(0);
    for (i, b) in buf.iter_mut().enumerate() {
        *b ^= scramble[i % 20];
    }
    buf
}

fn descramble_password(buf: &[u8], scramble: &[u8; 20]) -> String {
    let mut out = buf.to_vec();
    for (i, b) in out.iter_mut().enumerate() {
        *b ^= scramble[i % 20];
    }
    let end = out.iter().position(|&b| b == 0).unwrap_or(out.len());
    String::from_utf8_lossy(&out[..end]).into_owned()
}

/// Encrypt XOR-scrambled password with the server public key (client side / tests).
pub fn encrypt_password_rsa(
    public_key_pem: &[u8],
    password: &str,
    scramble: &[u8; 20],
) -> Result<Vec<u8>, rsa::errors::Error> {
    let pem_str = std::str::from_utf8(public_key_pem)
        .map_err(|_| rsa::errors::Error::Pkcs8(rsa::pkcs8::Error::KeyMalformed))?;
    let public_key = RsaPublicKey::from_public_key_pem(pem_str)
        .map_err(|_| rsa::errors::Error::Pkcs8(rsa::pkcs8::Error::KeyMalformed))?;
    let plain = scramble_password(password, scramble);
    let padding = Oaep::new::<Sha1>();
    let mut rng = rand::thread_rng();
    public_key.encrypt(&mut rng, padding, &plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsa_password_roundtrip() {
        let keys = CachingSha2RsaKeys::generate().unwrap();
        let scramble = [9u8; 20];
        let pem = keys.public_key_pem();
        let encrypted = encrypt_password_rsa(&pem, "secret", &scramble).unwrap();
        let decrypted = keys.decrypt_password(&encrypted, &scramble).unwrap();
        assert_eq!(decrypted, "secret");
    }
}
