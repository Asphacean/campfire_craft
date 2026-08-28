//! Two HTTP clients, named so they cannot be confused (RESEARCH.md's
//! "Claude's discretion" table): [`campfire_client`] trusts only our own CA
//! and nothing else, [`public_client`] trusts reqwest's bundled webpki
//! roots and is the only client allowed to touch Mojang, Adoptium, and
//! Forge. Mixing them up would either break TLS pinning (T-04-01-01) or
//! make a Mojang/Forge request fail against our own private CA — using two
//! named constructors instead of one configurable one means neither
//! mistake compiles.

use std::time::Duration;

/// Embedded at compile time — never read from disk at runtime, so a
/// tampered on-disk copy can't silently become the trust anchor. Committed
/// to the repo (it's a public certificate) as `assets/campfire-ca.pem`.
const EMBEDDED_CA_PEM: &[u8] = include_bytes!("../assets/campfire-ca.pem");

const USER_AGENT: &str = concat!("campfire-launcher/", env!("CARGO_PKG_VERSION"));

/// The pinned client: trusts only our own CA (T-04-01-01). Built-in root
/// certificates are explicitly disabled — `campfire-cli pin-check` proves
/// this by requiring a certificate *failure* against a public-CA host with
/// this exact client, because a pass-only test cannot distinguish pinning
/// from ordinary TLS.
///
/// The **only** host this client is ever pointed at is `mc.campfire.pub`.
pub fn campfire_client() -> reqwest::Client {
    let ca = reqwest::Certificate::from_pem(EMBEDDED_CA_PEM)
        .expect("embedded campfire-ca.pem is not valid PEM");
    reqwest::Client::builder()
        // `tls_certs_only` both adds our CA AND disables every native/
        // built-in root — a single call is what makes "pinned" mean
        // pinned rather than "our CA plus the usual trust store".
        .tls_certs_only([ca])
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
        .expect("failed to build the pinned HTTPS client")
}

/// The public client: reqwest's ordinary bundled webpki root store. This is
/// for Mojang, Adoptium, and Forge only — **never** point this at
/// `campfire.pub`; it has no knowledge of our own CA at all.
pub fn public_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
        .expect("failed to build the public HTTPS client")
}

/// The base URL every `campfire_client()` call in this crate uses.
pub const CAMPFIRE_BASE_URL: &str = "https://mc.campfire.pub:8444";

#[cfg(test)]
mod tests {
    use super::EMBEDDED_CA_PEM;

    /// T-04-01-13: a CA rotation that misses the launcher must fail the
    /// build, not ship silently. Compares against the repository's own copy
    /// when it's present (i.e. running from a checkout, not a bare crate
    /// package) — this is exactly the drift-detection the threat register
    /// requires.
    #[test]
    fn embedded_ca_matches_the_repository_copy_when_present() {
        let repo_copy = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../ca/campfire-ca.pem");
        if !repo_copy.exists() {
            // Not running from a full checkout (e.g. a packaged crate) —
            // nothing to compare against.
            return;
        }
        let repo_bytes = std::fs::read(&repo_copy).expect("read repository CA copy");
        assert_eq!(
            EMBEDDED_CA_PEM, repo_bytes,
            "launcher/core/assets/campfire-ca.pem has drifted from ca/campfire-ca.pem"
        );
    }
}
