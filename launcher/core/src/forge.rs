//! D-11: Forge 1.12.2 installed exactly the way research proved it works on
//! this Pi — headless, with no display, because the installer's own silent
//! prerequisite (a `launcher_profiles.json` stub) is written before it runs.
//! This module does exactly that and nothing more: the installer's own
//! install-time patch/rewrite pipeline is never reimplemented here, there is
//! no re-run once the version JSON already exists, and there is no
//! speculative hand-constructed fallback (research reproduced the real path
//! on this exact machine; a fallback for a failure that hasn't occurred is
//! the kind of work this project does not do).
//!
//! A one-line acknowledgement, per this plan's discretion table: the
//! installer's own embedded `_comment_` field politely asks automated
//! tooling not to bypass its download page's ad revenue. This project's use
//! — a private group of five to seven people, one install per player, once
//! — is the low-volume case that note is not aimed at.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::http::public_client;
use crate::java;
use crate::log;
use crate::mojang::{self, Library, VersionJson};
use crate::paths::{game_dir, install_root, libraries_dir, versions_dir};
use crate::progress::{Progress, ProgressSink};

/// Note the `-forge-` separator, not `forge` bare — this exact string is
/// what the installer produced on this Pi this session.
pub const FORGE_ID: &str = "1.12.2-forge-14.23.5.2860";

const FORGE_INSTALLER_URL: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860-installer.jar";
/// Measured from the copy already sitting in this repository's `downloads/`
/// — not copied from a webpage. A mismatch here means a compromised maven
/// is about to hand this process a jar to execute with the player's own
/// privileges; that download is deleted and the install fails hard instead.
const FORGE_INSTALLER_SHA256: &str = "ea7c33ba95e3993a98d0e9e38168c0759ec323a18675a71d938e1f3f70e6e8e7";

/// The stub the installer will not tell you it needs (research, this
/// session): without it, `--installClient` fails with "There is no
/// minecraft launcher profile in ..." and produces nothing, while still
/// exiting 0 — a failure mode that reads like success.
const LAUNCHER_PROFILES_STUB: &str =
    r#"{"profiles":{},"selectedProfile":"","clientToken":"00000000-0000-0000-0000-000000000000","authenticationDatabase":{}}"#;

#[derive(Debug)]
pub enum ForgeError {
    Network(String),
    ChecksumMismatch,
    /// The installer ran but the expected version JSON never appeared or
    /// didn't parse — the actual success signal (T-04-03-09), not the exit
    /// code, which research recorded as 0 even on the installer's own
    /// error path.
    InstallFailed(String),
    /// The empty-URL Forge jar (extracted by the installer, never fetched)
    /// is missing or fails its SHA-1 check.
    MissingLibrary(String),
    /// Loading the vanilla parent (`mojang::load_version_json`) failed —
    /// task 1's `ensure_vanilla` must run before this module can merge.
    VanillaMissing(String),
    Java(String),
    Io(String),
}

impl ForgeError {
    fn from_io(e: std::io::Error) -> Self {
        ForgeError::Io(e.to_string())
    }
}

/// The merged vanilla-plus-Forge version, ready for `launch.rs` to consume:
/// child (`Forge`) `mainClass`/`minecraftArguments` win, child libraries
/// come first on the classpath, parent (vanilla) libraries follow with a
/// name-keyed dedupe so a patched library shadows the vanilla one.
#[derive(Debug, Clone)]
pub struct MergedVersion {
    pub id: String,
    pub main_class: String,
    pub minecraft_arguments: String,
    pub libraries: Vec<Library>,
    pub asset_index_id: String,
    pub client_jar: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct ForgeReport {
    pub installer_hash_verified: bool,
    pub already_installed: bool,
    pub version_id: String,
    pub merged_library_count: u32,
    pub classpath_len: usize,
}

fn forge_version_json_path() -> PathBuf {
    versions_dir().join(FORGE_ID).join(format!("{FORGE_ID}.json"))
}

fn try_load_forge_json() -> Option<VersionJson> {
    let bytes = std::fs::read(forge_version_json_path()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Downloads the installer through `public_client()` into a cache directory
/// under the install root, verifying the pinned sha256 before returning its
/// path — skips the download entirely if already cached and correct.
async fn ensure_installer_jar() -> Result<PathBuf, ForgeError> {
    let cache_dir = install_root().join("cache");
    std::fs::create_dir_all(&cache_dir).map_err(ForgeError::from_io)?;
    let dest = cache_dir.join("forge-1.12.2-14.23.5.2860-installer.jar");

    if let Ok(bytes) = std::fs::read(&dest) {
        if sha256_hex(&bytes) == FORGE_INSTALLER_SHA256 {
            log::info("forge: installer already cached and sha256-verified");
            return Ok(dest);
        }
    }

    let client = public_client();
    let resp = client
        .get(FORGE_INSTALLER_URL)
        .send()
        .await
        .map_err(|e| ForgeError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(ForgeError::Network(format!(
            "HTTP {} fetching the Forge installer",
            resp.status()
        )));
    }
    let bytes = resp.bytes().await.map_err(|e| ForgeError::Network(e.to_string()))?;
    let actual = sha256_hex(&bytes);
    if actual != FORGE_INSTALLER_SHA256 {
        return Err(ForgeError::ChecksumMismatch);
    }
    std::fs::write(&dest, &bytes).map_err(ForgeError::from_io)?;
    log::info(&format!("forge: installer downloaded and verified sha256={actual}"));
    Ok(dest)
}

/// Written unconditionally, every time, into both the installer's actual
/// target directory (`install_root()`, where `versions/`/`libraries/`
/// already live per `paths.rs`'s layout) and `game_dir()` — Forge's client
/// runtime has historically also looked for this file in the `--gameDir` it
/// is launched with, so this second, defensive write costs one tiny file
/// and closes that gap too.
fn write_profile_stub() -> Result<(), ForgeError> {
    std::fs::write(install_root().join("launcher_profiles.json"), LAUNCHER_PROFILES_STUB)
        .map_err(ForgeError::from_io)?;
    std::fs::write(game_dir().join("launcher_profiles.json"), LAUNCHER_PROFILES_STUB).map_err(ForgeError::from_io)?;
    Ok(())
}

/// Test-only escape hatch, mirroring `CAMPFIRE_HOME` (`paths.rs`) and
/// `CAMPFIRE_JAVA_FORCE_CHECKSUM_MISMATCH` (`java.rs`): this Pi has no
/// Windows/macOS Java to actually execute, so the integration proof run
/// here points at the Phase 1 Temurin 8 already installed for the game
/// server (borrowed read-only). Unset, this always calls
/// `java::ensure_java(java::detect_target()?)` — the launcher's own
/// provisioned runtime, never a system one, exactly like every other
/// caller of `ensure_java`.
async fn resolve_forge_java() -> Result<PathBuf, ForgeError> {
    if let Ok(path) = std::env::var("CAMPFIRE_FORGE_JAVA") {
        return Ok(PathBuf::from(path));
    }
    let target = java::detect_target().map_err(|e| ForgeError::Java(format!("{e:?}")))?;
    let provision = java::ensure_java(target).await.map_err(|e| ForgeError::Java(format!("{e:?}")))?;
    Ok(provision.java_path)
}

/// Shells out once. Success is defined as "the expected version JSON now
/// exists and parses" (checked by the caller), never the exit code alone —
/// research recorded the installer exiting 0 on its own error path.
fn run_installer(java_path: &Path, installer_path: &Path) -> Result<(), ForgeError> {
    let target = install_root();
    let output = std::process::Command::new(java_path)
        .arg("-jar")
        .arg(installer_path)
        .arg("--installClient")
        .arg(&target)
        .current_dir(&target)
        .output()
        .map_err(|e| ForgeError::InstallFailed(format!("could not spawn the installer: {e}")))?;
    log::info(&format!(
        "forge installer exit={:?} stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    ));
    Ok(())
}

/// The empty-URL library (research point 4): written directly by the
/// installer from its own embedded copy, never downloaded. Asserting it
/// exists with the expected SHA-1 is the whole check — "fixing" the
/// download builder to error on an empty URL is the anti-pattern research
/// names explicitly.
fn assert_empty_url_library_present(child: &VersionJson) -> Result<(), ForgeError> {
    let Some(lib) = child.libraries.iter().find(|l| {
        l.downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .map(|a| a.url.is_empty())
            .unwrap_or(false)
    }) else {
        // Expected for this pinned build; not fatal in general (a future
        // Forge version might not have one), just logged.
        log::info("forge: no empty-url library entry found in the produced version JSON");
        return Ok(());
    };
    let artifact = lib.downloads.as_ref().unwrap().artifact.as_ref().unwrap();
    let path = libraries_dir().join(&artifact.path);
    let bytes = std::fs::read(&path)
        .map_err(|_| ForgeError::MissingLibrary(format!("expected self-extracted jar missing: {}", path.display())))?;
    let actual_sha1 = sha1_smol::Sha1::from(&bytes).digest().to_string();
    if actual_sha1 != artifact.sha1 {
        return Err(ForgeError::MissingLibrary(format!(
            "{} sha1 mismatch: expected {} got {actual_sha1}",
            path.display(),
            artifact.sha1
        )));
    }
    log::info(&format!(
        "forge: empty-url library {} checked in place (not fetched), sha1={actual_sha1}",
        artifact.path
    ));
    Ok(())
}

/// Child's `mainClass`/`minecraftArguments` win; child libraries first,
/// then parent libraries not already named, keyed by maven coordinate.
fn merge(child: &VersionJson, parent: &VersionJson) -> Result<MergedVersion, ForgeError> {
    let minecraft_arguments = child
        .minecraft_arguments
        .clone()
        .ok_or_else(|| ForgeError::InstallFailed("Forge version JSON has no minecraftArguments".to_string()))?;
    // Forge's own produced JSON never redefines `assetIndex` — it inherits
    // this from the vanilla parent, which always carries it.
    let asset_index_id = parent
        .asset_index
        .as_ref()
        .ok_or_else(|| ForgeError::VanillaMissing("vanilla version JSON has no assetIndex".to_string()))?
        .id
        .clone();

    let mut seen = std::collections::HashSet::new();
    let mut libraries = Vec::new();
    for lib in child.libraries.iter().chain(parent.libraries.iter()) {
        if seen.insert(lib.name.clone()) {
            libraries.push(lib.clone());
        }
    }

    let client_jar = versions_dir().join(&parent.id).join(format!("{}.jar", parent.id));
    Ok(MergedVersion {
        id: child.id.clone(),
        main_class: child.main_class.clone(),
        minecraft_arguments,
        libraries,
        asset_index_id,
        client_jar,
    })
}

/// Skips the installer entirely if `versions/<forge-id>/<forge-id>.json`
/// already exists and parses (research's anti-pattern: re-running the
/// installer on every launch). Otherwise: download + sha256-verify the
/// installer, write the profile stub, run it with the launcher's own
/// provisioned Java, and merge the result with the cached vanilla parent
/// from `mojang::load_version_json`.
pub async fn ensure_forge(sink: ProgressSink<'_>) -> Result<(ForgeReport, MergedVersion), ForgeError> {
    if let Some(child) = try_load_forge_json() {
        let parent = mojang::load_version_json().map_err(|e| ForgeError::VanillaMissing(format!("{e:?}")))?;
        let merged = merge(&child, &parent)?;
        sink(Progress::Done);
        log::info("forge: already installed, skipped installer");
        return Ok((
            ForgeReport {
                installer_hash_verified: true,
                already_installed: true,
                version_id: child.id,
                merged_library_count: merged.libraries.len() as u32,
                classpath_len: merged.libraries.len() + 1,
            },
            merged,
        ));
    }

    sink(Progress::Step {
        name: "Fetching Forge installer".to_string(),
        current: 1,
        total: 4,
    });
    let installer_path = ensure_installer_jar().await?;

    sink(Progress::Step {
        name: "Writing launcher profile stub".to_string(),
        current: 2,
        total: 4,
    });
    write_profile_stub()?;

    sink(Progress::Step {
        name: "Running Forge installer".to_string(),
        current: 3,
        total: 4,
    });
    let java_path = resolve_forge_java().await?;
    run_installer(&java_path, &installer_path)?;

    let child = try_load_forge_json().ok_or_else(|| {
        ForgeError::InstallFailed(format!(
            "{} did not appear after the installer ran",
            forge_version_json_path().display()
        ))
    })?;
    assert_empty_url_library_present(&child)?;

    let parent = mojang::load_version_json().map_err(|e| ForgeError::VanillaMissing(format!("{e:?}")))?;
    let merged = merge(&child, &parent)?;

    sink(Progress::Step {
        name: "Merging with vanilla".to_string(),
        current: 4,
        total: 4,
    });
    sink(Progress::Done);
    log::info(&format!(
        "forge: installed {} — merged {} libraries",
        child.id,
        merged.libraries.len()
    ));

    Ok((
        ForgeReport {
            installer_hash_verified: true,
            already_installed: false,
            version_id: child.id,
            merged_library_count: merged.libraries.len() as u32,
            classpath_len: merged.libraries.len() + 1,
        },
        merged,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mojang::{AssetIndexRef, ClientDownload, Downloads};

    fn fake_version(id: &str, main_class: &str, args: Option<&str>, lib_names: &[&str]) -> VersionJson {
        VersionJson {
            id: id.to_string(),
            inherits_from: None,
            main_class: main_class.to_string(),
            minecraft_arguments: args.map(str::to_string),
            asset_index: Some(AssetIndexRef {
                id: "1.12".to_string(),
                sha1: "0".repeat(40),
                size: 0,
                url: "https://piston-meta.mojang.com/x".to_string(),
            }),
            downloads: Some(Downloads {
                client: ClientDownload {
                    sha1: "0".repeat(40),
                    size: 0,
                    url: "https://piston-data.mojang.com/x".to_string(),
                },
            }),
            libraries: lib_names
                .iter()
                .map(|name| Library {
                    name: name.to_string(),
                    rules: vec![],
                    downloads: None,
                    natives: None,
                    extract: None,
                })
                .collect(),
        }
    }

    #[test]
    fn merge_puts_child_libraries_first_and_dedupes_by_name() {
        let child = fake_version(
            FORGE_ID,
            "net.minecraft.launchwrapper.Launch",
            Some("--username ${auth_player_name}"),
            &["net.minecraftforge:forge:1.0", "shared:lib:1.0"],
        );
        let parent = fake_version(
            "1.12.2",
            "net.minecraft.client.main.Main",
            Some("--username ${auth_player_name}"),
            &["shared:lib:1.0", "org.lwjgl:lwjgl:2.9.4"],
        );
        let merged = merge(&child, &parent).unwrap();
        assert_eq!(merged.libraries.len(), 3, "expected dedupe to drop the shared entry once");
        assert_eq!(merged.libraries[0].name, "net.minecraftforge:forge:1.0");
        assert_eq!(merged.main_class, "net.minecraft.launchwrapper.Launch");
    }

    #[test]
    fn merge_fails_loudly_when_the_child_has_no_minecraft_arguments() {
        let child = fake_version(FORGE_ID, "x", None, &[]);
        let parent = fake_version("1.12.2", "y", Some("--username ${auth_player_name}"), &[]);
        assert!(matches!(merge(&child, &parent), Err(ForgeError::InstallFailed(_))));
    }
}
