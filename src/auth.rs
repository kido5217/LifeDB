//! Argon2id password hashing (design D4): PHC strings, crate-default
//! parameters, and a dummy-hash path for timing-safe login attempts.

use std::sync::OnceLock;

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::Argon2;

/// Hash `password` with a fresh random salt; returns a PHC `$argon2id$...` string.
pub fn hash(password: &str) -> anyhow::Result<String> {
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|err| anyhow::anyhow!("hashing password: {err}"))?;
    Ok(hash.to_string())
}

/// Verify `password` against a stored PHC hash; false on any parse failure.
pub fn verify(password: &str, stored: &str) -> bool {
    match PasswordHash::new(stored) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// A valid, fixed PHC hash of a never-submitted password.
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash("timing-shield-placeholder").expect("dummy hash computes"))
}

/// Spend the same work on an unknown user as on a wrong password.
pub fn verify_against_dummy(password: &str) {
    let _ = verify(password, dummy_hash());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_verifies_and_wrong_password_rejects() {
        let stored = hash("correct horse battery staple").unwrap();
        assert!(verify("correct horse battery staple", &stored));
        assert!(!verify("Correct horse battery staple", &stored));
    }

    #[test]
    fn stored_hash_is_self_describing_argon2id() {
        let stored = hash("another fine passphrase").unwrap();
        assert!(stored.starts_with("$argon2id$"), "{stored}");
        let parsed = PasswordHash::new(&stored).expect("parses as PHC");
        assert_eq!(parsed.algorithm.as_str(), "argon2id");
        assert_eq!(parsed.version.map(|v| v.to_string()).as_deref(), Some("19"));
        let params = parsed.params.as_str();
        for expected in ["m=", "t=", "p="] {
            assert!(params.contains(expected), "params {params:?} lack {expected}");
        }
        assert!(parsed.salt.is_some() && parsed.hash.is_some(), "salt and hash present");
    }

    #[test]
    fn dummy_path_completes_verification_work() {
        verify_against_dummy("whatever");
    }
}
