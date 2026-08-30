//! LNCH-08: the startup self-update check against our own feed
//! (`/launcher/latest.json`), over the pinned CA — the feed is on our own
//! host, so it gets [`crate::http::campfire_client`] and nothing else, the
//! same trust anchor every other campfire.pub request in this crate uses.
//!
//! D-08/UI-SPEC's "error / update-check" row: **any failure at all** —
//! network, a non-2xx status, malformed JSON, an unparsable version string
//! on either side — returns `None` and logs a line. There is no dialog, no
//! banner and no retry loop for a failed check: self-update is a
//! convenience, never a precondition for playing.
//!
//! This module owns only the check. The actual signed download-and-install
//! goes through `tauri-plugin-updater`'s own `Updater`/`Update` types in
//! `src-tauri` — that plugin verifies the minisign signature this module
//! has no way to check itself; duplicating that logic here would be a
//! second, unaudited implementation of exactly the thing D-20 says must
//! stay official.

use serde::Deserialize;

use crate::http::{campfire_base_url, campfire_client};
use crate::log;

/// Only the fields this check actually reads. `tauri-plugin-updater`'s
/// schema has a `platforms` map too (url + signature per target) — this
/// struct doesn't need it, and `serde` ignores fields it doesn't declare,
/// so a real feed with `platforms` still deserializes here without error.
#[derive(Debug, Clone, Deserialize)]
struct Feed {
    version: String,
    #[serde(default)]
    notes: String,
}

/// What the startup check found: a version strictly newer than the one
/// running, plus whatever release notes the feed carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    pub version: String,
    pub notes: String,
}

/// Parses a `major.minor.patch` string into a tuple for numeric,
/// non-lexical comparison — `"0.10.0" > "0.9.0"` must hold, which a plain
/// string compare gets backwards (`"0.9.0" > "0.10.0"` lexically). Returns
/// `None` for anything that doesn't parse as three dot-separated integers;
/// a malformed version on either side must never be treated as "newer" —
/// that would turn a corrupt feed into a spurious update prompt instead of
/// the silent no-op the contract requires.
fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None; // a fourth component — not the three-part scheme this feed uses
    }
    Some((major, minor, patch))
}

/// `true` only when `candidate` parses and is strictly greater than
/// `current` under numeric `(major, minor, patch)` ordering. Any parse
/// failure on either side is `false`, not an error — the caller (`check`)
/// is already committed to "no failure here is ever loud".
fn is_newer(current: &str, candidate: &str) -> bool {
    match (parse_semver(current), parse_semver(candidate)) {
        (Some(cur), Some(cand)) => cand > cur,
        _ => false,
    }
}

/// Fetches `/launcher/latest.json` over the pinned CA and compares its
/// `version` against `current_version`. Returns `Some` only when the feed
/// is well-formed AND advertises something strictly newer; every other
/// outcome — the request failing, a non-success status, unparsable JSON,
/// an unparsable version on either side, or the feed matching/predating
/// the running build — is `None`, logged once, and otherwise silent.
pub async fn check(current_version: &str) -> Option<Available> {
    let url = format!("{}/launcher/latest.json", campfire_base_url());
    let resp = match campfire_client().get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            log::info(&format!("update-check: request failed (silent by contract): {e}"));
            return None;
        }
    };
    if !resp.status().is_success() {
        log::info(&format!(
            "update-check: feed returned {} (silent by contract)",
            resp.status()
        ));
        return None;
    }
    let feed: Feed = match resp.json().await {
        Ok(f) => f,
        Err(e) => {
            log::info(&format!("update-check: malformed feed body (silent by contract): {e}"));
            return None;
        }
    };
    if is_newer(current_version, &feed.version) {
        log::info(&format!(
            "update-check: {current_version} -> {} available",
            feed.version
        ));
        Some(Available {
            version: feed.version,
            notes: feed.notes,
        })
    } else {
        log::info(&format!(
            "update-check: running {current_version}, feed advertises {} — nothing to do",
            feed.version
        ));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_higher_minor_version_is_newer_even_with_a_smaller_string_ordering() {
        // "0.10.0" sorts before "0.9.0" as a plain string — the whole point
        // of parsing into integers rather than comparing strings.
        assert!(is_newer("0.9.0", "0.10.0"));
        assert!(!is_newer("0.10.0", "0.9.0"));
    }

    #[test]
    fn identical_versions_are_not_newer() {
        assert!(!is_newer("1.2.3", "1.2.3"));
    }

    #[test]
    fn a_patch_bump_is_newer() {
        assert!(is_newer("1.2.3", "1.2.4"));
    }

    #[test]
    fn a_malformed_candidate_is_never_newer() {
        assert!(!is_newer("1.0.0", "not-a-version"));
        assert!(!is_newer("1.0.0", ""));
        assert!(!is_newer("1.0.0", "1.0"));
        assert!(!is_newer("1.0.0", "1.0.0.1"));
    }

    #[test]
    fn a_malformed_current_is_never_older() {
        assert!(!is_newer("not-a-version", "1.0.0"));
    }
}
