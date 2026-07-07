//! Authentication plugins for MySQL wire protocol.

pub mod caching_sha2;
pub mod caching_sha2_rsa;
pub mod native;

pub use caching_sha2::{
    caching_sha2_fast_scramble, is_empty_password_auth, verify_caching_sha2_fast,
    AUTH_PLUGIN_CACHING_SHA2,
};
pub use caching_sha2_rsa::{
    auth_more_data_fast_auth_ok, auth_more_data_full_auth_required, auth_more_data_public_key,
    encrypt_password_rsa, is_public_key_request, CachingSha2RsaKeys,
    CACHING_SHA2_PUBLIC_KEY_REQUEST,
};
pub use native::{native_password_scramble, verify_native_password, AUTH_PLUGIN_NATIVE};

/// Verify handshake auth response for supported plugins.
pub fn verify_auth_response(
    password: &str,
    scramble: &[u8; 20],
    auth_response: &[u8],
    plugin: &str,
) -> bool {
    match plugin {
        AUTH_PLUGIN_CACHING_SHA2 => verify_caching_sha2_fast(password, scramble, auth_response),
        AUTH_PLUGIN_NATIVE => verify_native_password(password, scramble, auth_response),
        _ => false,
    }
}

/// Try client-declared plugin, then fall back to the other supported plugin.
pub fn verify_auth_with_fallback(
    password: &str,
    scramble: &[u8; 20],
    auth_response: &[u8],
    plugin: Option<&str>,
) -> bool {
    if let Some(p) = plugin {
        if verify_auth_response(password, scramble, auth_response, p) {
            return true;
        }
    }
    for p in [AUTH_PLUGIN_CACHING_SHA2, AUTH_PLUGIN_NATIVE] {
        if plugin == Some(p) {
            continue;
        }
        if verify_auth_response(password, scramble, auth_response, p) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_accepts_native_when_plugin_omitted() {
        let scramble = [7u8; 20];
        let response = native_password_scramble("pw", &scramble);
        assert!(verify_auth_with_fallback("pw", &scramble, &response, None));
    }
}
