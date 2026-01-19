use argon2::{
    Argon2,
    PasswordHash,
    PasswordVerifier,
    PasswordHasher,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};

use argon2::password_hash::{SaltString, rand_core::OsRng as PHOsRng};

/// Verify password using Argon2 hash stored in DB.
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    let parsed = match PasswordHash::new(stored_hash) {
        Ok(p) => p,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Hash a new password using Argon2id with a random salt.
/// Store the returned string in dcms_user.password_hash.
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut PHOsRng);
    let argon2 = Argon2::default();

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|phc| phc.to_string())
        .map_err(|e| format!("argon2 hash error: {e}"))
}

/// Generate an opaque session token to return to the client.
/// We store only a hash(token) in DB for safety.
pub fn generate_access_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Hash token for DB storage (SHA-256 hex).
pub fn hash_access_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let out = hasher.finalize();
    hex::encode(out)
}
