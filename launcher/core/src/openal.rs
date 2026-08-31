//! UAT gap-closure #5 (round 2): the operator's `hs_err` on the Mac pinned
//! the *actual* v0.1.9 crash — `SIGSEGV` at `Java_org_lwjgl_openal_EFX10_
//! nalGenFilters2` inside LWJGL2's own `liblwjgl.dylib`. RLCraft's sound
//! mods call OpenAL EFX (`alGenFilters`); Apple's system `OpenAL.framework`
//! (what macOS LWJGL2 resolves against) has never implemented EFX, so the
//! JNI stub calls a null extension-function pointer. This is an OpenAL
//! problem, not an architecture problem — it hits the x86_64-under-Rosetta
//! path exactly as much as a hypothetical arm64-native one, so the fix
//! applies to **both** shipped Mac targets, not just Apple Silicon.
//!
//! The fix: replace whatever `openal.dylib` LWJGL2 would otherwise load
//! with a real, EFX-capable OpenAL-soft build. Pinned to a specific
//! Homebrew core bottle rather than fetched from a version-floating API —
//! `openal-soft` is a small, stable library, and a hardcoded
//! version+digest is a smaller, more auditable surface than a second
//! `query_adoptium`-style live resolver for a single dylib.
//!
//! Source: Homebrew core's own build of openal-soft 1.25.2, published as an
//! OCI artifact on `ghcr.io` (Homebrew's official bottle host since the
//! `formulae.brew.sh` JSON API switched off bintray). Verified this session
//! by hand: `https://formulae.brew.sh/api/formula/openal-soft.json` names
//! this exact blob digest for the `sonoma` (Intel macOS 14, x86_64) bottle;
//! downloading it through the same two-call anonymous-token OCI flow this
//! module uses reproduced the identical SHA-256 and the extracted
//! `libopenal.1.25.2.dylib` is a genuine `Mach-O 64-bit x86_64` shared
//! library containing `AL/efx.h`'s symbol set. x86_64 (not arm64) is the
//! only architecture this project needs: both shipped Mac targets
//! provision an x86_64 Java 8 JRE (`java.rs`'s `spec_for` — Apple Silicon
//! runs it under Rosetta, D-10), so the JVM process — and therefore every
//! native library it `dlopen()`s, including this one — is x86_64 on both.

use std::path::PathBuf;

#[cfg(target_os = "macos")]
use std::path::Path;

#[cfg(target_os = "macos")]
use sha2::{Digest, Sha256};
#[cfg(target_os = "macos")]
use tokio::io::AsyncWriteExt;

#[cfg(target_os = "macos")]
use crate::http::public_client;
#[cfg(target_os = "macos")]
use crate::log;
#[cfg(target_os = "macos")]
use crate::paths::{install_root, io_ctx, unique_tmp_suffix};

/// Anonymous-pull token endpoint — the standard OCI Distribution flow every
/// public `ghcr.io` image/artifact uses; no credentials, no account.
/// Named as plain constants (not gated on macOS) so the module-doc-adjacent
/// unit tests below can assert against them on every platform this crate's
/// own `cargo test` runs on, including this Pi's Linux CI.
#[cfg(any(test, target_os = "macos"))]
const GHCR_TOKEN_URL: &str =
    "https://ghcr.io/token?service=ghcr.io&scope=repository:homebrew/core/openal-soft:pull";
#[cfg(any(test, target_os = "macos"))]
const GHCR_BLOB_URL: &str =
    "https://ghcr.io/v2/homebrew/core/openal-soft/blobs/sha256:56c3ef78464993c58c095113234b398b7f9cc42a87debf09eaf53ec992cdde36";
/// The blob's own SHA-256 — matches the digest named in the URL by
/// construction of content-addressed storage, but verified independently
/// here anyway rather than trusting the URL string alone (the same
/// never-extract-unverified discipline `java.rs::ensure_java` applies to
/// the Adoptium archive).
#[cfg(any(test, target_os = "macos"))]
const GHCR_BLOB_SHA256: &str = "56c3ef78464993c58c095113234b398b7f9cc42a87debf09eaf53ec992cdde36";
/// The one file this module needs out of the whole bottle tree.
#[cfg(target_os = "macos")]
const OPENAL_INNER_PATH: &str = "openal-soft/1.25.2/lib/libopenal.1.25.2.dylib";
#[cfg(target_os = "macos")]
const OPENAL_VERSION: &str = "1.25.2";

#[derive(Debug)]
pub enum OpenAlError {
    Network(String),
    ChecksumMismatch,
    Extract(String),
    Io(String),
}

impl From<std::io::Error> for OpenAlError {
    fn from(e: std::io::Error) -> Self {
        OpenAlError::Io(e.to_string())
    }
}

#[cfg(target_os = "macos")]
fn cache_path() -> PathBuf {
    install_root().join("openal-soft").join(format!("libopenal-{OPENAL_VERSION}.dylib"))
}

#[cfg(target_os = "macos")]
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(target_os = "macos")]
#[derive(serde::Deserialize)]
struct GhcrToken {
    token: String,
}

#[cfg(target_os = "macos")]
async fn ghcr_anonymous_token(client: &reqwest::Client) -> Result<String, OpenAlError> {
    let resp = client
        .get(GHCR_TOKEN_URL)
        .send()
        .await
        .map_err(|e| OpenAlError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(OpenAlError::Network(format!("ghcr token endpoint returned HTTP {}", resp.status())));
    }
    let parsed: GhcrToken = resp.json().await.map_err(|e| OpenAlError::Network(e.to_string()))?;
    Ok(parsed.token)
}

/// Downloads the pinned bottle blob (a `.tar.gz`), verifies its whole-file
/// SHA-256 against [`GHCR_BLOB_SHA256`] before touching the tar reader at
/// all, then extracts only [`OPENAL_INNER_PATH`] — mirrors
/// `java.rs::ensure_java`'s never-extract-unverified shape, scaled down to
/// one file instead of a whole runtime tree.
#[cfg(target_os = "macos")]
async fn fetch_and_extract(client: &reqwest::Client, dest: &Path) -> Result<(), OpenAlError> {
    let token = ghcr_anonymous_token(client).await?;
    let mut resp = client
        .get(GHCR_BLOB_URL)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.oci.image.layer.v1.tar+gzip")
        .send()
        .await
        .map_err(|e| OpenAlError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(OpenAlError::Network(format!("ghcr blob fetch returned HTTP {}", resp.status())));
    }

    let cache_dir = dest.parent().ok_or_else(|| OpenAlError::Io("no parent for openal cache path".to_string()))?;
    std::fs::create_dir_all(cache_dir).map_err(|e| OpenAlError::Io(io_ctx("create_dir_all", cache_dir, e)))?;
    let unique = unique_tmp_suffix();
    let tmp_archive = cache_dir.join(format!(".tmp-openal-{unique}.tar.gz"));

    let download: Result<String, OpenAlError> = async {
        let mut file = tokio::fs::File::create(&tmp_archive)
            .await
            .map_err(|e| OpenAlError::Io(io_ctx("create", &tmp_archive, e)))?;
        let mut hasher = Sha256::new();
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    hasher.update(&chunk);
                    file.write_all(&chunk)
                        .await
                        .map_err(|e| OpenAlError::Io(io_ctx("write", &tmp_archive, e)))?;
                }
                Ok(None) => break,
                Err(e) => return Err(OpenAlError::Network(e.to_string())),
            }
        }
        file.flush().await.map_err(|e| OpenAlError::Io(io_ctx("flush", &tmp_archive, e)))?;
        Ok(to_hex(&hasher.finalize()))
    }
    .await;

    let actual = match download {
        Ok(hash) => hash,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_archive);
            return Err(e);
        }
    };
    if actual != GHCR_BLOB_SHA256 {
        let _ = std::fs::remove_file(&tmp_archive);
        return Err(OpenAlError::ChecksumMismatch);
    }

    let extract_result = (|| -> Result<(), OpenAlError> {
        let file = std::fs::File::open(&tmp_archive).map_err(|e| OpenAlError::Io(io_ctx("open", &tmp_archive, e)))?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
        for entry in archive.entries().map_err(|e| OpenAlError::Extract(e.to_string()))? {
            let mut entry = entry.map_err(|e| OpenAlError::Extract(e.to_string()))?;
            let path = entry
                .path()
                .map_err(|e| OpenAlError::Extract(e.to_string()))?
                .to_string_lossy()
                .to_string();
            if path == OPENAL_INNER_PATH {
                let tmp_out = cache_dir.join(format!(".tmp-openal-out-{unique}"));
                let mut out = std::fs::File::create(&tmp_out).map_err(|e| OpenAlError::Io(io_ctx("create", &tmp_out, e)))?;
                std::io::copy(&mut entry, &mut out).map_err(|e| OpenAlError::Io(io_ctx("copy into", &tmp_out, e)))?;
                drop(out);
                std::fs::rename(&tmp_out, dest)
                    .map_err(|e| OpenAlError::Io(io_ctx(&format!("rename {} to", tmp_out.display()), dest, e)))?;
                return Ok(());
            }
        }
        Err(OpenAlError::Extract(format!("{OPENAL_INNER_PATH} not found in bottle archive")))
    })();
    let _ = std::fs::remove_file(&tmp_archive);
    extract_result
}

/// Fetches, verifies and caches the pinned openal-soft dylib, returning its
/// path either way (idempotent: skips the network entirely if already
/// cached). `#[cfg(not(target_os = "macos"))]` builds never call the real
/// implementation at all — see the stub below — so Windows/Linux builds
/// (and this Pi's own `cargo test`/CI) never make a `ghcr.io` request.
#[cfg(target_os = "macos")]
pub async fn ensure_openal_soft() -> Result<PathBuf, OpenAlError> {
    let dest = cache_path();
    if dest.is_file() {
        return Ok(dest);
    }
    let client = public_client();
    fetch_and_extract(&client, &dest).await?;
    log::info(&format!("openal: provisioned EFX-capable openal-soft {OPENAL_VERSION} at {}", dest.display()));
    Ok(dest)
}

/// Every non-macOS target: the EFX crash this module exists to fix is
/// macOS-only (Apple's `OpenAL.framework` is the thing missing EFX;
/// Windows/Linux LWJGL2 already bundle a real openal-soft). A compiled-out
/// no-op, not a runtime OS check, so the network path this module owns
/// literally cannot be reached from a non-macOS build.
#[cfg(not(target_os = "macos"))]
pub async fn ensure_openal_soft() -> Result<PathBuf, OpenAlError> {
    Err(OpenAlError::Extract("openal-soft provisioning is macOS-only".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghcr_urls_name_the_pinned_openal_soft_repo_and_digest() {
        assert!(GHCR_TOKEN_URL.contains("homebrew/core/openal-soft"));
        assert!(GHCR_BLOB_URL.ends_with(GHCR_BLOB_SHA256));
        assert_eq!(GHCR_BLOB_SHA256.len(), 64, "sha256 hex digest must be 64 chars");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cache_path_lives_under_the_install_root() {
        let _guard = crate::paths::CAMPFIRE_HOME_TEST_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("campfire-openal-test-{}", std::process::id()));
        unsafe {
            std::env::set_var("CAMPFIRE_HOME", &tmp);
        }
        assert!(cache_path().starts_with(install_root()));
        assert!(cache_path().to_string_lossy().contains(OPENAL_VERSION));
        unsafe {
            std::env::remove_var("CAMPFIRE_HOME");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The non-macOS stub must never attempt a network call — this test
    /// runs on this Pi's own Linux CI, so if the stub ever regressed into
    /// a real `ghcr.io` fetch, this would either hang past a sane test
    /// timeout or fail on a sandboxed CI runner with no network egress.
    #[cfg(not(target_os = "macos"))]
    #[tokio::test]
    async fn non_macos_stub_returns_immediately_without_a_network_call() {
        let err = ensure_openal_soft().await.unwrap_err();
        assert!(matches!(err, OpenAlError::Extract(_)));
    }
}
