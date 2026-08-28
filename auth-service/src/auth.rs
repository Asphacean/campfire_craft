//! Password/token cryptography. Argon2id via the `argon2` crate's default
//! features (0.6.0 API: `hash_password` generates its own salt and
//! `to_string()` yields the PHC string to store — no manual
//! `SaltString::generate(&mut OsRng)` ceremony, that was the 0.4/0.5-era
//! pattern). Tokens are 32 CSPRNG bytes, base64url (no padding) encoded, and
//! are themselves hashed with the same argon2id path before storage (D-03) —
//! the raw token exists only in the /login response and the caller's memory.

use std::sync::OnceLock;

use argon2::password_hash::phc::PasswordHash;
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

/// 32 random bytes is the token's entropy budget (D-03) — plenty for a
/// 12-hour-lived bearer credential with no online brute-force surface
/// (argon2id-hashed at rest, and never guessed remotely: /validate rejects
/// on the first failed attempt, there is no retry loop to exhaust).
const TOKEN_BYTES: usize = 32;

/// Hash a password or a raw token into a PHC string (`$argon2id$...`) for
/// storage. Fails only on an OOM-class RNG/allocation error, never on
/// caller-controlled input.
pub fn hash_secret(secret: &str) -> Result<String, argon2::password_hash::Error> {
    let argon2 = Argon2::default();
    Ok(argon2.hash_password(secret.as_bytes())?.to_string())
}

/// Verify a secret (password or raw token) against a stored PHC string.
/// Returns `false` on a genuine mismatch AND on a malformed stored hash —
/// callers must not distinguish the two, or a corrupt row becomes an oracle.
pub fn verify_secret(secret: &str, stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(secret.as_bytes(), &parsed)
        .is_ok()
}

/// Generate a new bearer token: `TOKEN_BYTES` from the OS CSPRNG, base64url
/// (no padding) encoded. `argon2`'s `password-hash` dependency only
/// re-exports `rand_core` behind a feature this crate does not enable by
/// default (confirmed by reading password-hash 0.6.1's `Cargo.toml`/`lib.rs`
/// directly), so this uses `getrandom` directly instead — already an
/// approved, verified crate (RESEARCH.md Package Legitimacy Audit sources)
/// and already a transitive dependency of `argon2`'s default `getrandom`
/// feature.
pub fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).expect("OS CSPRNG unavailable");
    URL_SAFE_NO_PAD.encode(bytes)
}

static DUMMY_HASH: OnceLock<String> = OnceLock::new();

/// A valid argon2id PHC string that verifies against no real secret.
/// `/login` runs the same argon2 verification cost against this hash for an
/// unknown nick as it does against a real user's stored hash — otherwise an
/// unknown-nick response would return measurably faster than a
/// wrong-password response, letting a caller enumerate registered nicks by
/// timing (T-02-01-01).
pub fn dummy_hash() -> &'static str {
    DUMMY_HASH.get_or_init(|| {
        hash_secret("dummy-password-never-a-real-account-credential")
            .expect("hashing the fixed dummy secret cannot fail")
    })
}
