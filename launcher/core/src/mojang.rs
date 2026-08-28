//! LNCH-04 / DIST-03: Minecraft's own files, from Minecraft's own hosts,
//! SHA-1 verified before anything is trusted. This module imports
//! [`crate::http::public_client`] and nothing else — the pinned-CA HTTP
//! client `auth.rs`/`manifest.rs` use for our own distribution host is
//! never named anywhere below, and a unit test asserts every constant URL
//! in this file is a Mojang or Minecraft host. That is DIST-03 enforced by
//! module structure, not by review.
//!
//! Mojang's own hashes are **SHA-1**, a different domain from the pack
//! manifest's SHA-256 (`manifest.rs`) — this module uses `sha1_smol`
//! exclusively and never touches `sha2`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use futures_util::StreamExt;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

use crate::http::public_client;
use crate::log;
use crate::paths::{assets_dir, libraries_dir, versions_dir};
use crate::progress::{Progress, ProgressSink};

/// The only two Mojang/Minecraft hosts this module ever names as a
/// constant — every other URL it fetches is one the version manifest or
/// version JSON told it to, and is itself hosted on `*.mojang.com` or
/// `*.minecraft.net` (asserted structurally by the acceptance criteria's
/// live download, not re-checked host-by-host here).
pub const VERSION_MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const RESOURCES_BASE_URL: &str = "https://resources.download.minecraft.net";

/// The one Minecraft version this whole project targets (RLCraft 1.12.2).
pub const MC_VERSION: &str = "1.12.2";

/// Small library batches (natives + jars): a handful of jars off a Pi.
const LIBRARY_CONCURRENCY: usize = 4;
/// The asset index is ~3,700 tiny files against Mojang's CDN — a different
/// shape of work, so it gets its own, higher concurrency (RESEARCH.md /
/// plan discretion table).
const ASSET_CONCURRENCY: usize = 8;

#[derive(Debug)]
pub enum MojangError {
    Network(String),
    HashMismatch { what: String },
    NotFound(String),
    Json(String),
    Io(String),
}

impl MojangError {
    fn from_io(e: std::io::Error) -> Self {
        MojangError::Io(e.to_string())
    }
}

#[derive(Debug, Clone, Default)]
pub struct VanillaReport {
    pub version_id: String,
    pub libraries_included: u32,
    pub libraries_excluded: u32,
    pub natives_resolved: u32,
    pub asset_index_id: String,
    pub asset_object_count: u32,
    pub bytes_downloaded: u64,
}

// ---------------------------------------------------------------------
// Version JSON shape — shared with `forge.rs`, which parses the exact same
// struct out of the installer's produced file and merges it with this
// module's cached vanilla copy.
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    /// Empty for exactly one Forge library (the self-extracted Forge jar,
    /// `forge.rs`'s special case) — never fetched when empty, only checked.
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    /// Maps an OS name (`windows`/`osx`/`linux`) to a classifier key (e.g.
    /// `natives-windows`), possibly containing a `${arch}` placeholder.
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default)]
    pub extract: Option<Extract>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientDownload {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Downloads {
    pub client: ClientDownload,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default)]
    pub inherits_from: Option<String>,
    pub main_class: String,
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    pub asset_index: AssetIndexRef,
    pub downloads: Downloads,
    #[serde(default)]
    pub libraries: Vec<Library>,
}

// ---------------------------------------------------------------------
// The platform rule engine — shared by this module's own download filter
// and by `launch.rs`'s classpath/natives builder, which walks the same
// merged library list a second time at launch. One evaluation, not two.
// ---------------------------------------------------------------------

/// `windows`/`osx`/`linux` — Mojang's own three `os.name` values, mapped
/// from `std::env::consts::OS` (which reports `macos`, not `osx`).
pub(crate) fn current_os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "osx",
        other => other,
    }
}

/// Standard Mojang rule evaluation: no rules at all means included; with
/// rules present, the last one whose `os.name` matches (or has no `os`
/// predicate at all) decides, default excluded. Getting this wrong shows up
/// as a missing native at launch, not as a download-time error.
pub(crate) fn rule_allows(rules: &[Rule], os_name: &str) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let applies = match &rule.os {
            None => true,
            Some(os) => match &os.name {
                None => true,
                Some(name) => name == os_name,
            },
        };
        if applies {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

/// Resolves a library's `natives` map entry for the current platform,
/// substituting `${arch}` where present (32/64-bit, per pointer width).
pub(crate) fn resolve_native_classifier(natives: &HashMap<String, String>, os_name: &str) -> Option<String> {
    natives.get(os_name).map(|v| {
        let arch = if cfg!(target_pointer_width = "64") { "64" } else { "32" };
        v.replace("${arch}", arch)
    })
}

// ---------------------------------------------------------------------
// Hashing + the shared download-verify-rename path
// ---------------------------------------------------------------------

fn sha1_hex(bytes: &[u8]) -> String {
    sha1_smol::Sha1::from(bytes).digest().to_string()
}

fn sha1_of_file(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(sha1_hex(&bytes))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), MojangError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(MojangError::from_io)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(MojangError::from_io)?;
    std::fs::rename(&tmp, path).map_err(MojangError::from_io)?;
    Ok(())
}

async fn fetch_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, MojangError> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| MojangError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(MojangError::Network(format!("HTTP {} fetching {url}", resp.status())));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| MojangError::Network(e.to_string()))
}

/// Streams `url` to a same-directory temp file, verifying SHA-1 before the
/// atomic rename; skips entirely if `dest` already exists with the right
/// hash (the idempotence every acceptance criterion here relies on).
async fn download_sha1_verified(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha1: &str,
) -> Result<u64, MojangError> {
    if let Some(existing) = sha1_of_file(dest) {
        if existing == expected_sha1 {
            return Ok(0);
        }
    }
    let parent = dest
        .parent()
        .ok_or_else(|| MojangError::Io(format!("no parent directory for {}", dest.display())))?;
    std::fs::create_dir_all(parent).map_err(MojangError::from_io)?;

    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| MojangError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(MojangError::Network(format!("HTTP {} fetching {url}", resp.status())));
    }

    let file_name = dest.file_name().and_then(|n| n.to_str()).unwrap_or("download");
    let tmp = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));

    let result: Result<(u64, String), MojangError> = async {
        let mut file = tokio::fs::File::create(&tmp).await.map_err(MojangError::from_io)?;
        let mut hasher = sha1_smol::Sha1::new();
        let mut size = 0u64;
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    hasher.update(&chunk);
                    size += chunk.len() as u64;
                    file.write_all(&chunk).await.map_err(MojangError::from_io)?;
                }
                Ok(None) => break,
                Err(e) => return Err(MojangError::Network(e.to_string())),
            }
        }
        file.flush().await.map_err(MojangError::from_io)?;
        Ok((size, hasher.digest().to_string()))
    }
    .await;

    let (size, actual) = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(e);
        }
    };
    if actual != expected_sha1 {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(MojangError::HashMismatch {
            what: dest.display().to_string(),
        });
    }
    tokio::fs::rename(&tmp, dest).await.map_err(MojangError::from_io)?;
    Ok(size)
}

// ---------------------------------------------------------------------
// The chain: version manifest -> version JSON -> client jar -> libraries
// -> assets. Every step through `public_client()` only.
// ---------------------------------------------------------------------

#[derive(Deserialize)]
struct VersionManifestEntry {
    id: String,
    url: String,
    sha1: String,
}

#[derive(Deserialize)]
struct VersionManifestRaw {
    versions: Vec<VersionManifestEntry>,
}

pub fn version_json_path() -> PathBuf {
    versions_dir().join(MC_VERSION).join(format!("{MC_VERSION}.json"))
}

/// Reads and parses the cached vanilla version JSON — `forge.rs` calls this
/// to load the `inheritsFrom` parent for its merge. Fails if `ensure_vanilla`
/// hasn't run yet.
pub fn load_version_json() -> Result<VersionJson, MojangError> {
    let bytes = std::fs::read(version_json_path()).map_err(MojangError::from_io)?;
    serde_json::from_slice(&bytes).map_err(|e| MojangError::Json(e.to_string()))
}

/// Fetches the version manifest and the pinned `1.12.2` version JSON,
/// SHA-1-verifying the version JSON against the manifest's own published
/// hash before caching it. Cheap (a few hundred KB) — always re-fetched
/// rather than trusting a stale cache, unlike the large binary artifacts
/// below which skip re-download when already present and correct.
async fn ensure_version_json(client: &reqwest::Client) -> Result<VersionJson, MojangError> {
    let manifest_bytes = fetch_bytes(client, VERSION_MANIFEST_URL).await?;
    let manifest: VersionManifestRaw =
        serde_json::from_slice(&manifest_bytes).map_err(|e| MojangError::Json(e.to_string()))?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == MC_VERSION)
        .ok_or_else(|| MojangError::NotFound(format!("{MC_VERSION} not in version manifest")))?;

    let version_bytes = fetch_bytes(client, &entry.url).await?;
    let actual = sha1_hex(&version_bytes);
    if actual != entry.sha1 {
        return Err(MojangError::HashMismatch {
            what: format!("{MC_VERSION}.json"),
        });
    }
    write_atomic(&version_json_path(), &version_bytes)?;
    serde_json::from_slice(&version_bytes).map_err(|e| MojangError::Json(e.to_string()))
}

#[derive(Deserialize)]
struct AssetObject {
    hash: String,
    /// Not read — objects are verified by SHA-1 (the filename itself), not
    /// by size; kept only because the index JSON always carries it.
    #[allow(dead_code)]
    size: u64,
}

#[derive(Deserialize)]
struct AssetIndexFile {
    objects: HashMap<String, AssetObject>,
}

struct DownloadJob {
    dest: PathBuf,
    url: String,
    sha1: String,
}

/// Runs `jobs` at most `concurrency` at a time, reporting a `Step` per
/// completion and aborting on the first error — mirrors `manifest.rs`'s
/// `sync()` shape (pin the batch, fail whole on the first bad hash).
async fn run_download_batch(
    client: &reqwest::Client,
    jobs: Vec<DownloadJob>,
    concurrency: usize,
    step_name: &str,
    sink: ProgressSink<'_>,
) -> Result<u64, MojangError> {
    let total = jobs.len() as u32;
    let counter = AtomicU32::new(0);
    let bytes_total = AtomicU64::new(0);

    let results: Vec<Result<u64, MojangError>> = futures_util::stream::iter(jobs.into_iter().map(|job| {
        let client = client;
        let counter = &counter;
        let bytes_total = &bytes_total;
        async move {
            let r = download_sha1_verified(client, &job.url, &job.dest, &job.sha1).await;
            let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
            sink(Progress::Step {
                name: step_name.to_string(),
                current: done,
                total,
            });
            if let Ok(size) = r {
                bytes_total.fetch_add(size, Ordering::Relaxed);
            }
            r
        }
    }))
    .buffer_unordered(concurrency)
    .collect()
    .await;

    for r in &results {
        if let Err(e) = r {
            return Err(match e {
                MojangError::HashMismatch { what } => MojangError::HashMismatch { what: what.clone() },
                MojangError::Network(m) => MojangError::Network(m.clone()),
                MojangError::NotFound(m) => MojangError::NotFound(m.clone()),
                MojangError::Json(m) => MojangError::Json(m.clone()),
                MojangError::Io(m) => MojangError::Io(m.clone()),
            });
        }
    }
    Ok(bytes_total.load(Ordering::Relaxed))
}

/// The whole vanilla bootstrap: version JSON, client jar, rule-filtered
/// libraries and natives, and the full asset tree — every file SHA-1
/// verified against Mojang's own published hash, through `public_client()`
/// only. This function, and this whole file, has no way to reach our own
/// distribution host at all.
pub async fn ensure_vanilla(sink: ProgressSink<'_>) -> Result<VanillaReport, MojangError> {
    let client = public_client();

    sink(Progress::Step {
        name: "Fetching version manifest".to_string(),
        current: 1,
        total: 4,
    });
    let version = ensure_version_json(&client).await?;

    sink(Progress::Step {
        name: "Fetching client jar".to_string(),
        current: 2,
        total: 4,
    });
    let jar_dest = versions_dir().join(&version.id).join(format!("{}.jar", version.id));
    let mut bytes_downloaded =
        download_sha1_verified(&client, &version.downloads.client.url, &jar_dest, &version.downloads.client.sha1)
            .await?;

    // --- Libraries + natives, rule-filtered for the current platform ---
    let os_name = current_os_name();
    let mut libraries_included = 0u32;
    let mut libraries_excluded = 0u32;
    let mut natives_resolved = 0u32;
    let mut lib_jobs: Vec<DownloadJob> = Vec::new();

    for lib in &version.libraries {
        if !rule_allows(&lib.rules, os_name) {
            libraries_excluded += 1;
            continue;
        }
        libraries_included += 1;
        let Some(downloads) = &lib.downloads else { continue };
        if let Some(artifact) = &downloads.artifact {
            if !artifact.url.is_empty() {
                lib_jobs.push(DownloadJob {
                    dest: libraries_dir().join(&artifact.path),
                    url: artifact.url.clone(),
                    sha1: artifact.sha1.clone(),
                });
            }
        }
        if let Some(natives_map) = &lib.natives {
            if let Some(classifier_key) = resolve_native_classifier(natives_map, os_name) {
                if let Some(classifiers) = &downloads.classifiers {
                    if let Some(artifact) = classifiers.get(&classifier_key) {
                        lib_jobs.push(DownloadJob {
                            dest: libraries_dir().join(&artifact.path),
                            url: artifact.url.clone(),
                            sha1: artifact.sha1.clone(),
                        });
                        natives_resolved += 1;
                    }
                }
            }
        }
    }

    bytes_downloaded += run_download_batch(&client, lib_jobs, LIBRARY_CONCURRENCY, "Libraries", sink).await?;

    // --- Assets: index + every object ---
    sink(Progress::Step {
        name: "Fetching asset index".to_string(),
        current: 3,
        total: 4,
    });
    let index_bytes = fetch_bytes(&client, &version.asset_index.url).await?;
    let actual_index_sha1 = sha1_hex(&index_bytes);
    if actual_index_sha1 != version.asset_index.sha1 {
        return Err(MojangError::HashMismatch {
            what: format!("asset index {}", version.asset_index.id),
        });
    }
    let index_path = assets_dir().join("indexes").join(format!("{}.json", version.asset_index.id));
    write_atomic(&index_path, &index_bytes)?;
    let index: AssetIndexFile = serde_json::from_slice(&index_bytes).map_err(|e| MojangError::Json(e.to_string()))?;

    sink(Progress::Step {
        name: "Fetching assets".to_string(),
        current: 4,
        total: 4,
    });
    let mut asset_jobs: Vec<DownloadJob> = Vec::with_capacity(index.objects.len());
    for obj in index.objects.values() {
        let prefix = &obj.hash[0..2];
        asset_jobs.push(DownloadJob {
            dest: assets_dir().join("objects").join(prefix).join(&obj.hash),
            url: format!("{RESOURCES_BASE_URL}/{prefix}/{}", obj.hash),
            sha1: obj.hash.clone(),
        });
    }
    let asset_object_count = asset_jobs.len() as u32;
    bytes_downloaded += run_download_batch(&client, asset_jobs, ASSET_CONCURRENCY, "Assets", sink).await?;

    sink(Progress::Done);
    log::info(&format!(
        "vanilla: version={} libs_included={libraries_included} libs_excluded={libraries_excluded} natives={natives_resolved} assets={asset_object_count} bytes={bytes_downloaded}",
        version.id
    ));

    Ok(VanillaReport {
        version_id: version.id,
        libraries_included,
        libraries_excluded,
        natives_resolved,
        asset_index_id: version.asset_index.id,
        asset_object_count,
        bytes_downloaded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_constant_url_is_a_mojang_or_minecraft_host() {
        for url in [VERSION_MANIFEST_URL, RESOURCES_BASE_URL] {
            let host = url.trim_start_matches("https://").split('/').next().unwrap();
            assert!(
                host.ends_with("mojang.com") || host.ends_with("minecraft.net"),
                "not a Mojang/Minecraft host: {host}"
            );
        }
    }

    #[test]
    fn rule_engine_defaults_to_allowed_with_no_rules() {
        assert!(rule_allows(&[], "linux"));
    }

    #[test]
    fn rule_engine_excludes_a_platform_not_named_by_an_allow_rule() {
        let rules = vec![Rule {
            action: "allow".to_string(),
            os: Some(OsRule {
                name: Some("windows".to_string()),
            }),
        }];
        assert!(!rule_allows(&rules, "linux"));
        assert!(rule_allows(&rules, "windows"));
    }

    #[test]
    fn rule_engine_last_match_wins() {
        let rules = vec![
            Rule {
                action: "allow".to_string(),
                os: None,
            },
            Rule {
                action: "disallow".to_string(),
                os: Some(OsRule {
                    name: Some("osx".to_string()),
                }),
            },
        ];
        assert!(rule_allows(&rules, "linux"));
        assert!(!rule_allows(&rules, "osx"));
    }

    #[test]
    fn native_classifier_substitutes_arch_placeholder() {
        let mut natives = HashMap::new();
        natives.insert("windows".to_string(), "natives-windows-${arch}".to_string());
        let resolved = resolve_native_classifier(&natives, "windows").unwrap();
        assert!(resolved == "natives-windows-64" || resolved == "natives-windows-32");
        assert!(!resolved.contains("${arch}"));
    }
}
