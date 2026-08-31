//! D-15/D-16: the launch line, built as data so it can be asserted on a
//! machine that cannot run it. Natives extraction, the offline UUID, the
//! classpath, the two `-D` token-handoff properties, and the seeded server
//! list all live here; `spawn` is a separate call, deliberately, so the
//! command vector `build_launch_command` returns is the thing every
//! acceptance criterion in this plan actually checks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use md5::{Digest, Md5};

use crate::auth::Session;
use crate::forge::MergedVersion;
use crate::log;
use crate::mojang;
use crate::paths::{assets_dir, game_dir, io_ctx, libraries_dir, log_path, runtime_dir, versions_dir};

/// `mc.campfire.pub:25565` — the Minecraft server itself, distinct from the
/// HTTPS distribution point (`mc.campfire.pub:8444`). No autoconnect
/// placeholder exists in either version's argument template (research,
/// verified live), so the seeded server list below is the primary route
/// onto this server; these two arguments are a free attempt at something
/// better, never the only path.
const SERVER_HOST: &str = "mc.campfire.pub";
const SERVER_PORT: &str = "25565";

/// The precomputed one-entry NBT blob for `servers.dat` — one server, one
/// name, one address, all fixed for the life of this project, so a static
/// blob is fifteen bytes of code where an NBT writer would be a crate or a
/// hundred lines for a file with exactly one possible value. Generated and
/// round-trip-verified by a throwaway script; see 04-03-SUMMARY.md.
const SERVERS_DAT: &[u8] = include_bytes!("../assets/servers.dat");

/// Aikar-family G1 flags scaled for a modded client, plus the two
/// Forge-1.12.2-specific properties research names for a client pointed at
/// a private, non-Mojang-session server. One named constant (plan
/// discretion table): if the operator's real launch needs simpler flags,
/// this is the one line that changes.
const JVM_FLAGS: &[&str] = &[
    "-XX:+UseG1GC",
    "-XX:+ParallelRefProcEnabled",
    "-XX:MaxGCPauseMillis=200",
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+DisableExplicitGC",
    "-XX:+AlwaysPreTouch",
    "-XX:G1NewSizePercent=30",
    "-XX:G1MaxNewSizePercent=40",
    "-XX:G1HeapRegionSize=8M",
    "-XX:G1ReservePercent=20",
    "-XX:G1HeapWastePercent=5",
    "-XX:G1MixedGCCountTarget=4",
    "-XX:InitiatingHeapOccupancyPercent=15",
    "-XX:G1MixedGCLiveThresholdPercent=90",
    "-XX:G1RSetUpdatingPauseTimePercent=5",
    "-XX:SurvivorRatio=32",
    "-XX:+PerfDisableSharedMem",
    "-XX:MaxTenuringThreshold=1",
    "-Dfml.ignoreInvalidMinecraftCertificates=true",
    "-Dfml.ignorePatchDiscrepancies=true",
];

#[derive(Debug)]
pub enum LaunchError {
    /// A classpath entry (library or the vanilla client jar) that the
    /// merged version JSON names but which doesn't exist on disk.
    MissingClasspathEntry(String),
    /// The substitution map has no entry for a `${...}` token the
    /// `minecraftArguments` template used — a loud error, not a literal
    /// dollar sign reaching the game.
    UnknownPlaceholder(String),
    /// The builder refuses any java path outside `runtime_dir()` — the
    /// launcher's own provisioned runtime, never a system one.
    JavaOutsideRuntime,
    Extract(String),
    /// Always built via `paths::io_ctx` — the operation and path an
    /// `io::Error` failed on (gap-closure #4).
    Io(String),
}

/// The version-3 UUID Minecraft's offline mode uses:
/// `UUID.nameUUIDFromBytes(("OfflinePlayer:" + nick).getBytes(UTF_8))` — a
/// raw MD5 digest of the exact nick bytes with the version nibble set to 3
/// and the variant bits set to the RFC 4122 standard pattern. The nick must
/// be the exact casing the auth service returned (D-16): a differently
/// cased nick produces a different UUID here, which is the silent
/// data-loss mechanism the casing rule exists to prevent.
pub fn offline_uuid(nick: &str) -> String {
    let input = format!("OfflinePlayer:{nick}");
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest);
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn natives_dir(forge_id: &str) -> PathBuf {
    versions_dir().join(forge_id).join("natives")
}

/// Extracts every natives archive resolved for the current platform (per
/// `merged.libraries`) into `versions/<forge-id>/natives/`, applying the
/// shared archive-entry path guard (`java::assert_safe_archive_entry`,
/// T-04-03-04) to every entry and skipping directories and each library's
/// own `extract.exclude` list. Idempotent: skips entirely if the directory
/// is already populated.
pub fn extract_natives(merged: &MergedVersion) -> Result<PathBuf, LaunchError> {
    let dest = natives_dir(&merged.id);
    let already_populated = std::fs::read_dir(&dest)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    if already_populated {
        return Ok(dest);
    }
    std::fs::create_dir_all(&dest).map_err(|e| LaunchError::Io(io_ctx("create_dir_all", &dest, e)))?;

    let os_name = mojang::current_os_name();
    for lib in &merged.libraries {
        if !mojang::rule_allows(&lib.rules, os_name) {
            continue;
        }
        let Some(natives_map) = &lib.natives else { continue };
        let Some(classifier_key) = mojang::resolve_native_classifier(natives_map, os_name) else {
            continue;
        };
        let Some(downloads) = &lib.downloads else { continue };
        let Some(classifiers) = &downloads.classifiers else { continue };
        let Some(artifact) = classifiers.get(&classifier_key) else { continue };
        let archive_path = libraries_dir().join(&artifact.path);
        if !archive_path.is_file() {
            // Not fetched — shouldn't happen if `ensure_vanilla` ran, but
            // extraction has nothing to do if the archive never arrived.
            continue;
        }
        let exclude: &[String] = lib.extract.as_ref().map(|e| e.exclude.as_slice()).unwrap_or(&[]);
        extract_native_archive(&archive_path, &dest, exclude)?;
    }
    Ok(dest)
}

fn extract_native_archive(archive: &Path, dest: &Path, exclude: &[String]) -> Result<(), LaunchError> {
    let file = std::fs::File::open(archive).map_err(|e| LaunchError::Io(io_ctx("open", archive, e)))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| LaunchError::Extract(e.to_string()))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| LaunchError::Extract(e.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if exclude.iter().any(|prefix| name.starts_with(prefix.as_str())) {
            continue;
        }
        crate::java::assert_safe_archive_entry(&name).map_err(|e| LaunchError::Extract(format!("{e:?}")))?;
        let out_path = dest.join(&name);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| LaunchError::Io(io_ctx("create_dir_all", parent, e)))?;
        }
        let mut out = std::fs::File::create(&out_path).map_err(|e| LaunchError::Io(io_ctx("create", &out_path, e)))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| LaunchError::Io(io_ctx("copy into", &out_path, e)))?;
    }
    Ok(())
}

/// Every merged library allowed on this platform, in order, followed by the
/// vanilla client jar — the classpath is real only if every entry actually
/// exists on disk, checked here rather than merely counted.
fn build_classpath(merged: &MergedVersion) -> Result<Vec<PathBuf>, LaunchError> {
    let os_name = mojang::current_os_name();
    let mut entries = Vec::new();
    for lib in &merged.libraries {
        if !mojang::rule_allows(&lib.rules, os_name) {
            continue;
        }
        let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else {
            continue;
        };
        let path = libraries_dir().join(&artifact.path);
        if !path.is_file() {
            return Err(LaunchError::MissingClasspathEntry(path.display().to_string()));
        }
        entries.push(path);
    }
    if !merged.client_jar.is_file() {
        return Err(LaunchError::MissingClasspathEntry(merged.client_jar.display().to_string()));
    }
    entries.push(merged.client_jar.clone());
    Ok(entries)
}

/// Writes the seeded one-entry server list exactly once — if `servers.dat`
/// already exists (a player's own reordered/added list), it is never
/// touched again on any later launch.
pub fn seed_server_list() {
    let dest = game_dir().join("servers.dat");
    if dest.exists() {
        return;
    }
    if std::fs::write(&dest, SERVERS_DAT).is_ok() {
        log::info("seeded servers.dat (fresh install) — mc.campfire.pub");
    }
}

fn substitute_arguments(template: &str, map: &HashMap<&str, String>) -> Result<Vec<String>, LaunchError> {
    let mut out = Vec::new();
    for token in template.split_whitespace() {
        if let Some(key) = token.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
            let value = map
                .get(key)
                .ok_or_else(|| LaunchError::UnknownPlaceholder(key.to_string()))?;
            out.push(value.clone());
        } else {
            out.push(token.to_string());
        }
    }
    Ok(out)
}

/// Replaces every occurrence of `token` in the joined command line with the
/// redaction marker before writing it to `launcher.log` — the nick, the
/// flags, the classpath length and the arguments are exactly what an
/// operator needs when a friend pastes their log into chat, and the token
/// is exactly what must not be in it.
fn log_launch_command(argv: &[String], token: &str) {
    let joined = argv
        .iter()
        .map(|a| if token.is_empty() || !a.contains(token) { a.clone() } else { a.replace(token, "<redacted>") })
        .collect::<Vec<_>>()
        .join(" ");
    log::info(&format!("launch command: {joined}"));
}

/// Builds the complete `java` argument vector for RLCraft, in order: the
/// provisioned java path (must live under `runtime_dir()`), heap flags, the
/// JVM flag constant, the two token-handoff system properties
/// (`campfire.nick`/`campfire.token` — the whole contract
/// `ClientAuthHandler.buildResponse()` reads), the natives path, the
/// classpath, the main class, then the substituted game arguments, and
/// finally the two optional autoconnect arguments if `autoconnect` is true
/// — the only removable part of this whole command line.
pub fn build_launch_command(
    session: &Session,
    ram_gb: f32,
    merged: &MergedVersion,
    java_path: &Path,
    autoconnect: bool,
) -> Result<Vec<String>, LaunchError> {
    if !java_path.starts_with(runtime_dir()) {
        return Err(LaunchError::JavaOutsideRuntime);
    }

    let natives_path = extract_natives(merged)?;
    let classpath = build_classpath(merged)?;
    let cp_string = std::env::join_paths(&classpath)
        .map_err(|e| LaunchError::Io(e.to_string()))?
        .to_string_lossy()
        .to_string();

    // WR-05: `f32::clamp` panics on a NaN bound but does not sanitize a NaN
    // `self` — `f32::NAN.clamp(3.0, 10.0)` returns `NaN` unchanged, and
    // `NaN as u64` silently saturates to 0, producing `-Xms0M -Xmx0M`. The
    // Tauri `play` command clamps to [3.0, 10.0] but not against NaN;
    // `campfire-cli`'s `--ram` parses "nan" successfully and applies no
    // clamp at all. Guarded once here — the boundary every caller funnels
    // through — rather than in each caller.
    let ram_gb = if ram_gb.is_finite() { ram_gb.clamp(3.0, 10.0) } else { 3.0 };

    // The RAM slider moves in half-gigabyte steps (D-06), and the JVM's
    // own `-Xmx`/`-Xms` flags reject a fractional `G` suffix outright
    // (confirmed live on this Pi: `-Xmx7.5G` is "Invalid maximum heap
    // size") — megabytes is the smallest unit that represents a half
    // gigabyte as a whole number.
    let ram_mb = (ram_gb * 1024.0).round() as u64;
    let mut argv = vec![java_path.to_string_lossy().to_string()];
    argv.push(format!("-Xms{ram_mb}M"));
    argv.push(format!("-Xmx{ram_mb}M"));
    argv.extend(JVM_FLAGS.iter().map(|s| s.to_string()));
    argv.push(format!("-Dcampfire.nick={}", session.nick));
    argv.push(format!("-Dcampfire.token={}", session.token));
    argv.push(format!("-Djava.library.path={}", natives_path.display()));
    argv.push("-cp".to_string());
    argv.push(cp_string);
    argv.push(merged.main_class.clone());

    let mut map: HashMap<&str, String> = HashMap::new();
    map.insert("auth_player_name", session.nick.clone());
    map.insert("version_name", merged.id.clone());
    map.insert("game_directory", game_dir().display().to_string());
    map.insert("assets_root", assets_dir().display().to_string());
    map.insert("assets_index_name", merged.asset_index_id.clone());
    map.insert("auth_uuid", offline_uuid(&session.nick));
    map.insert("auth_access_token", "0".to_string());
    map.insert("user_type", "legacy".to_string());
    map.insert("version_type", "Forge".to_string());
    argv.extend(substitute_arguments(&merged.minecraft_arguments, &map)?);

    if autoconnect {
        argv.push("--server".to_string());
        argv.push(SERVER_HOST.to_string());
        argv.push("--port".to_string());
        argv.push(SERVER_PORT.to_string());
    }

    log_launch_command(&argv, &session.token);
    Ok(argv)
}

/// Spawns the built command with the game directory as its working
/// directory, redirecting both output streams into `launcher.log`. Kept
/// separate from `build_launch_command` so the command line is assertable
/// on a machine — this one — that cannot run it at all.
pub fn spawn(argv: &[String]) -> std::io::Result<std::process::Child> {
    let log = log_path();
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| std::io::Error::new(e.kind(), io_ctx("open", &log, e)))?;
    let err = out.try_clone()?;
    std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(game_dir())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| std::io::Error::new(e.kind(), io_ctx("spawn", Path::new(&argv[0]), e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_the_known_fixed_nick_vector() {
        // Computed independently in Python via hashlib.md5 + the same
        // version/variant bit patch, this session — see 04-03-SUMMARY.md.
        assert_eq!(offline_uuid("TestNick"), "0df37fa9-fe90-3132-a7e6-a995becc802f");
    }

    #[test]
    fn two_casings_of_the_same_nick_produce_different_uuids() {
        assert_ne!(offline_uuid("TestNick"), offline_uuid("testnick"));
        assert_ne!(offline_uuid("TestNick"), offline_uuid("TESTNICK"));
    }
}
