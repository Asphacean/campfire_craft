//! LNCH-02: the client pack sync, ported faithfully from
//! `scripts/assemble-client.py` — the reviewed, live-tested reference this
//! module is a translation of, not a redesign. Fetch `manifest.json` once
//! over the pinned CA, reject the whole manifest (not one bad entry) if any
//! path/url fails the guard, diff by size+sha256, download only what
//! changed to a same-directory temp file before an atomic rename, apply the
//! cumulative `delete[]` list, seed the pack's own client options exactly
//! once, and never touch player state — twice: once because the manifest
//! contract already excludes it, and once again in code, for the day that
//! contract slips.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::http::{campfire_base_url, campfire_client};
use crate::log;
use crate::progress::Progress;

/// D-19: four downloads at a time, over the one shared client. A Pi
/// serving one friend group doesn't want more, and a constant needs no
/// configuration surface.
const DOWNLOAD_CONCURRENCY: usize = 4;

/// DIST-03: our own host never serves the Minecraft client jar, libraries
/// or assets — a manifest path under any of these, or naming the vanilla
/// client jar, is a hard rejection of the whole manifest.
const FORBIDDEN_PREFIXES: [&str; 3] = ["libraries/", "assets/", "versions/"];

/// The manifest is contractually incapable of naming these (`docs/DIST-OPS.md`);
/// this is the second lock, enforced in code, for the day the contract slips.
const NEVER_TOUCH_TOP_LEVEL_FILES: [&str; 3] = ["options.txt", "optionsof.txt", "servers.dat"];
const NEVER_TOUCH_DIRS: [&str; 4] = ["saves", "screenshots", "logs", "crash-reports"];

/// Leave letters, digits and `_.-~` alone (matches Python's `quote()`
/// default "always safe" set); percent-encode everything else per segment,
/// including the spaces several real pack filenames contain — the '/'
/// separators themselves are never touched because each segment is encoded
/// independently and rejoined.
const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'.')
    .remove(b'-')
    .remove(b'_')
    .remove(b'~');

const OPTIONS_TXT: &[u8] = include_bytes!("../assets/options.txt");
const OPTIONSOF_TXT: &[u8] = include_bytes!("../assets/optionsof.txt");

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub pack_version: String,
    pub mc: String,
    pub forge: String,
    pub java: u32,
    pub files: Vec<ManifestFile>,
    #[serde(default)]
    pub delete: Vec<String>,
}

/// A variant, not a formatted sentence — `strings.rs`/the UI map these to
/// the Copywriting Contract's actual error sentences.
#[derive(Debug)]
pub enum SyncError {
    Network(String),
    /// The whole manifest was refused by the guard — never a per-entry skip.
    ManifestRejected(String),
    HashMismatch { path: String },
    DiskFull,
    Permission(String),
    Io(String),
}

impl SyncError {
    fn from_io(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::StorageFull => SyncError::DiskFull,
            std::io::ErrorKind::PermissionDenied => SyncError::Permission(e.to_string()),
            _ => SyncError::Io(e.to_string()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    pub checked: u32,
    pub downloaded: u32,
    pub deleted: u32,
    pub seeded: u32,
    pub bytes_downloaded: u64,
}

#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    pub checked: u32,
    pub repaired: u32,
}

fn looks_like_minecraft_client_jar(basename: &str) -> bool {
    let lowered = basename.to_lowercase();
    lowered.starts_with("minecraft") && lowered.ends_with(".jar")
}

/// Lexical join only — no filesystem access, so it works even when `rel`'s
/// final component doesn't exist yet (a real `canonicalize` would fail on a
/// file that hasn't been downloaded). Safe to call only after the absolute
/// and parent-component checks have already rejected anything that could
/// escape via a `..` or a leading `/`; kept as an explicit second check
/// anyway, for the day one of those checks is weakened without this one
/// being revisited.
fn lexically_join(base: &Path, rel: &str) -> PathBuf {
    let mut result = base.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            std::path::Component::Normal(part) => result.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                // Already caught by the leading-'/' check for a value that
                // starts this way; treat any other root-ish component
                // mid-string as an escape attempt too.
                result.push("\u{0}forbidden-root-component");
            }
        }
    }
    result
}

fn validate_field(value: &str, real_dest: &Path) -> Result<(), SyncError> {
    if value.starts_with('/') {
        return Err(SyncError::ManifestRejected(format!(
            "manifest path/url is absolute: {value}"
        )));
    }
    if value.split('/').any(|c| c == "..") {
        return Err(SyncError::ManifestRejected(format!(
            "manifest path/url contains a parent-directory component: {value}"
        )));
    }
    if value.chars().any(|c| (c as u32) < 0x20 || (c as u32) == 0x7F) {
        return Err(SyncError::ManifestRejected(format!(
            "manifest path/url contains a control character: {value}"
        )));
    }
    let joined = lexically_join(real_dest, value);
    if !joined.starts_with(real_dest) {
        return Err(SyncError::ManifestRejected(format!(
            "manifest path/url resolves outside the game directory: {value}"
        )));
    }
    Ok(())
}

/// The client-side path guard (T-04-02-01/T-04-02-06), ported faithfully
/// from `scripts/assemble-client.py`'s `validate_manifest_entries`. A
/// single bad entry rejects the **whole** manifest — the sync never begins
/// — because a per-entry skip would let one bad entry hide among hundreds
/// of good ones.
pub fn validate(manifest: &Manifest, game_dir: &Path) -> Result<(), SyncError> {
    let real_dest = std::fs::canonicalize(game_dir).unwrap_or_else(|_| game_dir.to_path_buf());
    for entry in &manifest.files {
        validate_field(&entry.path, &real_dest)?;
        validate_field(&entry.url, &real_dest)?;

        let basename = entry.path.rsplit('/').next().unwrap_or(&entry.path);
        if FORBIDDEN_PREFIXES.iter().any(|p| entry.path.starts_with(p))
            || looks_like_minecraft_client_jar(basename)
        {
            return Err(SyncError::ManifestRejected(format!(
                "DIST-03 violated — manifest references a Minecraft client/library/asset path: {}",
                entry.path
            )));
        }
    }
    for path in &manifest.delete {
        if path.starts_with('/') || path.split('/').any(|c| c == "..") {
            return Err(SyncError::ManifestRejected(format!(
                "delete[] entry fails the path guard: {path}"
            )));
        }
    }
    Ok(())
}

/// Parses and validates required fields structurally: `path`/`url`/`sha256`/
/// `size` are non-optional in [`ManifestFile`], so a manifest missing any
/// of them fails to deserialize — the same whole-manifest rejection as
/// every other guard rule, not a per-entry skip.
pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, SyncError> {
    serde_json::from_slice(bytes).map_err(|e| {
        SyncError::ManifestRejected(format!(
            "manifest is not valid JSON or is missing a required field: {e}"
        ))
    })
}

pub async fn fetch_manifest() -> Result<Manifest, SyncError> {
    let resp = campfire_client()
        .get(format!("{}/manifest.json", campfire_base_url()))
        .send()
        .await
        .map_err(|e| SyncError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(SyncError::Network(format!(
            "manifest fetch returned HTTP {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| SyncError::Network(e.to_string()))?;
    parse_manifest(&bytes)
}

/// The second lock: even though the manifest contract already excludes
/// player state from `files` and `delete`, no write or unlink in this
/// module ever touches one of these paths regardless of what a manifest
/// claims.
fn assert_never_touch(rel_path: &str) -> Result<(), SyncError> {
    let top = rel_path.split('/').next().unwrap_or("");
    if NEVER_TOUCH_DIRS.contains(&top) || NEVER_TOUCH_TOP_LEVEL_FILES.contains(&rel_path) {
        return Err(SyncError::ManifestRejected(format!(
            "refusing to touch protected path: {rel_path}"
        )));
    }
    Ok(())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Streaming, one-megabyte-chunk sha256 — the same chunk size
/// `scripts/assemble-client.py` uses.
fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(to_hex(&hasher.finalize()))
}

fn is_up_to_date(game_dir: &Path, entry: &ManifestFile) -> bool {
    let dest = game_dir.join(&entry.path);
    let Ok(meta) = std::fs::metadata(&dest) else {
        return false;
    };
    if meta.len() != entry.size {
        return false;
    }
    matches!(sha256_file(&dest), Ok(hash) if hash == entry.sha256)
}

fn percent_encode_path(url_part: &str) -> String {
    url_part
        .split('/')
        .map(|seg| utf8_percent_encode(seg, PATH_SEGMENT_ENCODE_SET).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Downloads one entry to a temporary file in its own destination
/// directory (same filesystem, so the final rename is atomic), verifying
/// size and sha256 before the rename. Shared by `sync` (concurrent) and
/// `verify` (sequential repair) — one download path, not two.
async fn download_one(
    client: &reqwest::Client,
    game_dir: &Path,
    entry: &ManifestFile,
) -> Result<u64, SyncError> {
    assert_never_touch(&entry.path)?;
    let dest_path = game_dir.join(&entry.path);
    let parent = dest_path
        .parent()
        .ok_or_else(|| SyncError::ManifestRejected(format!("no parent directory for {}", entry.path)))?;
    std::fs::create_dir_all(parent).map_err(SyncError::from_io)?;

    let url = format!(
        "{}/pack/{}",
        campfire_base_url(),
        percent_encode_path(&entry.url)
    );
    let mut resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| SyncError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(SyncError::Network(format!(
            "HTTP {} fetching {}",
            resp.status(),
            entry.path
        )));
    }

    let file_name = dest_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download");
    let tmp_path = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));

    let write_result: Result<(u64, String), SyncError> = async {
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(SyncError::from_io)?;
        let mut hasher = Sha256::new();
        let mut size: u64 = 0;
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    hasher.update(&chunk);
                    size += chunk.len() as u64;
                    file.write_all(&chunk).await.map_err(SyncError::from_io)?;
                }
                Ok(None) => break,
                Err(e) => return Err(SyncError::Network(e.to_string())),
            }
        }
        file.flush().await.map_err(SyncError::from_io)?;
        Ok((size, to_hex(&hasher.finalize())))
    }
    .await;

    let (size, actual_hash) = match write_result {
        Ok(v) => v,
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(e);
        }
    };

    if size != entry.size || actual_hash != entry.sha256 {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(SyncError::HashMismatch {
            path: entry.path.clone(),
        });
    }

    tokio::fs::rename(&tmp_path, &dest_path)
        .await
        .map_err(SyncError::from_io)?;
    Ok(size)
}

fn prune_empty_ancestors(removed: &Path, stop_at: &Path) {
    let mut dir = removed.parent().map(Path::to_path_buf);
    while let Some(d) = dir {
        if d == *stop_at {
            break;
        }
        match std::fs::read_dir(&d) {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    break;
                }
                if std::fs::remove_dir(&d).is_err() {
                    break;
                }
                dir = d.parent().map(Path::to_path_buf);
            }
            Err(_) => break,
        }
    }
}

/// Removes any locally-present file named in the cumulative `delete[]`
/// list, then prunes directories left empty by the removal. Never removes
/// anything not named there.
fn apply_deletes(manifest: &Manifest, game_dir: &Path) -> Result<u32, SyncError> {
    let mut deleted = 0u32;
    for path in &manifest.delete {
        assert_never_touch(path)?;
        let target = game_dir.join(path);
        if target.is_file() {
            std::fs::remove_file(&target).map_err(SyncError::from_io)?;
            log::info(&format!("deleted (per manifest delete[]): {path}"));
            deleted += 1;
            prune_empty_ancestors(&target, game_dir);
        }
    }
    Ok(deleted)
}

/// Seeds the pack's own tuned client options on a fresh install
/// (`docs/DIST-OPS.md` gap 1) — once, and never again: if either file
/// already exists, it is left completely alone on every later sync.
fn seed_options(game_dir: &Path) -> std::io::Result<u32> {
    let mut seeded = 0u32;
    let options_path = game_dir.join("options.txt");
    if !options_path.exists() {
        std::fs::write(&options_path, OPTIONS_TXT)?;
        log::info("seeded options.txt (fresh install)");
        seeded += 1;
    }
    let optionsof_path = game_dir.join("optionsof.txt");
    if !optionsof_path.exists() {
        std::fs::write(&optionsof_path, OPTIONSOF_TXT)?;
        log::info("seeded optionsof.txt (fresh install)");
        seeded += 1;
    }
    Ok(seeded)
}

fn pack_version_cache_path() -> PathBuf {
    crate::paths::install_root().join("pack_version.txt")
}

/// Caches the manifest's own `pack_version` after a successful sync, so
/// the version footer can show it on a cold start without waiting for the
/// next sync to complete.
fn cache_pack_version(version: &str) {
    let _ = std::fs::write(pack_version_cache_path(), version);
}

/// What the version footer reads: the `pack_version` the last *successful*
/// sync actually saw, or `None` before the first sync ever completes.
pub fn cached_pack_version() -> Option<String> {
    std::fs::read_to_string(pack_version_cache_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// One concurrent download slot's whole unit of work: download, report a
/// `Step` and (on success) a `Bytes` tick. A **named** `async fn`, not a
/// closure returning an inline `async move` block — the closure form,
/// passed straight to `Iterator::map`/`buffer_unordered`, tripped a known
/// rustc HRTB false-positive ("implementation of `FnOnce` is not general
/// enough") the moment this whole call chain was wrapped by
/// `tauri::generate_handler!`'s command dispatch macro; a plain named
/// `async fn` produces a concrete, unambiguous opaque `Future` type that
/// doesn't hit it.
#[allow(clippy::too_many_arguments)]
async fn download_one_and_report(
    client: &reqwest::Client,
    game_dir: &Path,
    entry: ManifestFile,
    downloaded_bytes: &AtomicU64,
    file_counter: &AtomicU32,
    start: std::time::Instant,
    download_total: u32,
    bytes_total: u64,
    sink: crate::progress::ProgressSink,
) -> Result<u64, SyncError> {
    let result = download_one(client, game_dir, &entry).await;
    let done = file_counter.fetch_add(1, Ordering::Relaxed) + 1;
    match &result {
        Ok(size) => {
            let so_far = downloaded_bytes.fetch_add(*size, Ordering::Relaxed) + size;
            let elapsed = start.elapsed().as_secs_f64().max(0.001);
            sink(Progress::Step {
                name: "Downloading".to_string(),
                current: done,
                total: download_total,
            });
            sink(Progress::Bytes {
                downloaded: so_far,
                total: bytes_total,
                per_sec: (so_far as f64 / elapsed) as u64,
            });
        }
        Err(_) => {
            sink(Progress::Step {
                name: "Downloading".to_string(),
                current: done,
                total: download_total,
            });
        }
    }
    result
}

/// The full sync: fetch and pin the manifest for the whole run, validate
/// it whole (reject, don't skip), diff by size+sha256, download only what
/// changed (at most [`DOWNLOAD_CONCURRENCY`] at a time), apply `delete[]`,
/// and seed the pack's own client options once.
pub async fn sync(sink: crate::progress::ProgressSink) -> Result<SyncReport, SyncError> {
    use futures_util::StreamExt;

    let game_dir = crate::paths::game_dir();
    log::info("sync: fetching manifest.json");
    let manifest = fetch_manifest().await?;
    validate(&manifest, &game_dir)?;
    log::info(&format!(
        "sync: manifest pinned — pack_version={} files={}",
        manifest.pack_version,
        manifest.files.len()
    ));

    let total_files = manifest.files.len() as u32;
    // Owned clones, not `&ManifestFile` borrowed from `manifest.files` —
    // a `Vec<&T>` fed through `Stream::map(closure).buffer_unordered()`
    // where the closure also captures other references (`client`,
    // `game_dir`) tripped a genuine, documented rustc/futures HRTB
    // limitation ("implementation of `FnOnce`/`Send` is not general
    // enough") the moment the result needed a `Send` bound proven for it
    // — both `tokio::spawn` and `tauri::generate_handler!`'s command
    // dispatch require exactly that proof. Owned items have no borrowed
    // lifetime tied to this Vec for the HRTB check to trip on; a
    // `ManifestFile` clone (a few small `String`s) is a trivial cost next
    // to the download itself.
    let mut to_download: Vec<ManifestFile> = Vec::new();
    for (i, entry) in manifest.files.iter().enumerate() {
        sink(Progress::Step {
            name: "Checking files".to_string(),
            current: i as u32 + 1,
            total: total_files,
        });
        if !is_up_to_date(&game_dir, entry) {
            to_download.push(entry.clone());
        }
    }

    let client = campfire_client();
    let download_total = to_download.len() as u32;
    let bytes_total: u64 = to_download.iter().map(|e| e.size).sum();
    let downloaded_bytes = AtomicU64::new(0);
    let file_counter = AtomicU32::new(0);
    let start = std::time::Instant::now();

    let results: Vec<Result<u64, SyncError>> = futures_util::stream::iter(to_download.into_iter().map(|entry| {
        download_one_and_report(
            &client,
            &game_dir,
            entry,
            &downloaded_bytes,
            &file_counter,
            start,
            download_total,
            bytes_total,
            sink.clone(),
        )
    }))
    .buffer_unordered(DOWNLOAD_CONCURRENCY)
    .collect()
    .await;

    let mut downloaded = 0u32;
    let mut bytes_downloaded = 0u64;
    let mut first_err: Option<SyncError> = None;
    for r in results {
        match r {
            Ok(size) => {
                downloaded += 1;
                bytes_downloaded += size;
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
    }
    if let Some(e) = first_err {
        sink(Progress::Failed {
            code: format!("{e:?}"),
        });
        return Err(e);
    }

    let deleted = apply_deletes(&manifest, &game_dir)?;
    let seeded = seed_options(&game_dir).map_err(SyncError::from_io)?;
    cache_pack_version(&manifest.pack_version);

    sink(Progress::Done);
    log::info(&format!(
        "sync complete: checked={total_files} downloaded={downloaded} deleted={deleted} seeded={seeded} bytes={bytes_downloaded}"
    ));
    Ok(SyncReport {
        checked: total_files,
        downloaded,
        deleted,
        seeded,
        bytes_downloaded,
    })
}

/// Re-hashes every managed file against the pinned manifest and repairs
/// (re-downloads) whatever is missing or mismatched, through the same
/// download path `sync` uses — not a parallel implementation. Sequential:
/// this is the "Verify files" button, not the hot Play-press path, and
/// simplicity wins over four-at-a-time speed here.
pub async fn verify(sink: crate::progress::ProgressSink) -> Result<VerifyReport, SyncError> {

    let game_dir = crate::paths::game_dir();
    let manifest = fetch_manifest().await?;
    validate(&manifest, &game_dir)?;
    apply_deletes(&manifest, &game_dir)?;

    let client = campfire_client();
    let total = manifest.files.len() as u32;
    let mut checked = 0u32;
    let mut repaired = 0u32;
    for (i, entry) in manifest.files.iter().enumerate() {
        sink(Progress::Step {
            name: "Verifying".to_string(),
            current: i as u32 + 1,
            total,
        });
        checked += 1;
        if !is_up_to_date(&game_dir, entry) {
            download_one(&client, &game_dir, entry).await?;
            repaired += 1;
            log::info(&format!("verify: repaired {}", entry.path));
        }
    }
    let _ = seed_options(&game_dir);
    cache_pack_version(&manifest.pack_version);
    sink(Progress::Done);
    Ok(VerifyReport { checked, repaired })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "campfire-manifest-unit-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn empty_manifest(delete: Vec<&str>) -> Manifest {
        Manifest {
            pack_version: "test".to_string(),
            mc: "1.12.2".to_string(),
            forge: "14.23.5.2860".to_string(),
            java: 8,
            files: vec![],
            delete: delete.into_iter().map(str::to_string).collect(),
        }
    }

    #[test]
    fn delete_removes_a_listed_file_and_prunes_the_now_empty_directory() {
        let dir = scratch_dir("delete");
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        std::fs::write(dir.join("mods/Stale.jar"), b"stale").unwrap();

        let manifest = empty_manifest(vec!["mods/Stale.jar"]);
        let deleted = apply_deletes(&manifest, &dir).unwrap();

        assert_eq!(deleted, 1);
        assert!(!dir.join("mods/Stale.jar").exists());
        assert!(
            !dir.join("mods").exists(),
            "the now-empty mods/ directory should be pruned"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The manifest contract already excludes player state from `delete[]`
    /// entirely — this proves the second lock, enforced in code, refuses a
    /// manifest that claimed otherwise too, rather than trusting the
    /// contract alone.
    #[test]
    fn delete_never_touches_player_state_even_if_a_manifest_claimed_it() {
        let dir = scratch_dir("delete-never-touch");
        std::fs::create_dir_all(dir.join("saves/World")).unwrap();
        std::fs::write(dir.join("saves/World/level.dat"), b"world").unwrap();

        let manifest = empty_manifest(vec!["saves/World/level.dat"]);
        let result = apply_deletes(&manifest, &dir);

        assert!(matches!(result, Err(SyncError::ManifestRejected(_))));
        assert!(dir.join("saves/World/level.dat").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `sync()` calls `validate()` on the exact struct this test builds,
    /// before any download or write happens — a rejection here structurally
    /// guarantees zero files land in the game directory, independent of
    /// whether the hostile manifest arrived over a real network round-trip.
    #[test]
    fn a_hostile_manifest_among_189_good_entries_is_rejected_before_any_file_would_be_written() {
        let dir = scratch_dir("hostile-e2e");
        let mut files: Vec<ManifestFile> = (0..189)
            .map(|i| ManifestFile {
                path: format!("mods/Good{i}.jar"),
                sha256: "0".repeat(64),
                size: 1,
                url: format!("mods/Good{i}.jar"),
            })
            .collect();
        files.push(ManifestFile {
            path: "../../../../etc/campfire-owned".to_string(),
            sha256: "0".repeat(64),
            size: 1,
            url: "../../../../etc/campfire-owned".to_string(),
        });
        let manifest = Manifest {
            pack_version: "test".to_string(),
            mc: "1.12.2".to_string(),
            forge: "14.23.5.2860".to_string(),
            java: 8,
            files,
            delete: vec![],
        };

        let result = validate(&manifest, &dir);
        match result {
            Err(SyncError::ManifestRejected(msg)) => {
                assert!(msg.contains("etc/campfire-owned"), "message was: {msg}")
            }
            other => panic!("expected the whole manifest to be rejected, got {other:?}"),
        }
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "no file should exist in the game directory after a rejected manifest"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
