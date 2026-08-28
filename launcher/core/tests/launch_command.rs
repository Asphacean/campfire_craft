//! The launch-line regression suite (LNCH-04): UUID derivation, argument
//! shape, classpath completeness and the token-redaction rule — all without
//! spawning a game. Each test gets its own scratch `CAMPFIRE_HOME` so tests
//! never share state or race on the process-wide environment variable.

use std::path::PathBuf;
use std::sync::Mutex;

use campfire_launcher_core::auth::Session;
use campfire_launcher_core::forge::MergedVersion;
use campfire_launcher_core::launch::{build_launch_command, offline_uuid};
use campfire_launcher_core::mojang::{Artifact, Library, LibraryDownloads};
use campfire_launcher_core::paths::{game_dir, libraries_dir, log_path, runtime_dir};

// `CAMPFIRE_HOME` is a process-wide env var; `cargo test` runs these in
// parallel threads by default, so every test that touches it takes this
// lock for its whole body — the same convention `paths.rs`'s own doctest
// would need if it ran concurrently with these.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct ScratchHome {
    _guard: std::sync::MutexGuard<'static, ()>,
    path: PathBuf,
}

impl ScratchHome {
    fn new(name: &str) -> Self {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::env::temp_dir().join(format!("campfire-launch-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        // SAFETY: serialized by ENV_LOCK across every test in this file.
        unsafe {
            std::env::set_var("CAMPFIRE_HOME", &path);
        }
        Self { _guard: guard, path }
    }
}

impl Drop for ScratchHome {
    fn drop(&mut self) {
        // SAFETY: still holding ENV_LOCK.
        unsafe {
            std::env::remove_var("CAMPFIRE_HOME");
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_dummy_file(path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, b"dummy").unwrap();
}

/// A minimal but real `MergedVersion` — one ordinary library (present on
/// disk) and the vanilla client jar (also present on disk), a template
/// using every placeholder `build_launch_command` must substitute, and no
/// natives (so `extract_natives` is a no-op instead of needing a real zip).
fn fake_merged(id: &str) -> MergedVersion {
    let lib_path = "com/example/dummy/1.0/dummy-1.0.jar";
    write_dummy_file(&libraries_dir().join(lib_path));
    let client_jar = client_jar_path(id);
    write_dummy_file(&client_jar);

    MergedVersion {
        id: id.to_string(),
        main_class: "net.minecraft.launchwrapper.Launch".to_string(),
        minecraft_arguments: "--username ${auth_player_name} --version ${version_name} \
             --gameDir ${game_directory} --assetsDir ${assets_root} \
             --assetIndex ${assets_index_name} --uuid ${auth_uuid} \
             --accessToken ${auth_access_token} --userType ${user_type} \
             --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker \
             --versionType ${version_type}"
            .to_string(),
        libraries: vec![Library {
            name: "com.example:dummy:1.0".to_string(),
            rules: vec![],
            downloads: Some(LibraryDownloads {
                artifact: Some(Artifact {
                    path: lib_path.to_string(),
                    sha1: String::new(),
                    size: 5,
                    url: String::new(),
                }),
                classifiers: None,
            }),
            natives: None,
            extract: None,
        }],
        asset_index_id: "1.12".to_string(),
        client_jar,
    }
}

fn client_jar_path(id: &str) -> PathBuf {
    campfire_launcher_core::paths::versions_dir().join(id).join(format!("{id}.jar"))
}

fn fake_session(nick: &str, token: &str) -> Session {
    Session {
        nick: nick.to_string(),
        token: token.to_string(),
        expires: 0,
    }
}

/// Points the builder at a fake "provisioned" java under `runtime_dir()` —
/// the builder only checks the path prefix, never that the file is
/// actually executable, which is exactly what makes it assertable here.
fn fake_java_path() -> PathBuf {
    let path = runtime_dir().join("fake-jdk").join("bin").join("java");
    write_dummy_file(&path);
    path
}

#[test]
fn fixed_nick_produces_the_expected_offline_uuid() {
    let _home = ScratchHome::new("uuid-fixed");
    assert_eq!(offline_uuid("TestNick"), "0df37fa9-fe90-3132-a7e6-a995becc802f");
}

#[test]
fn two_casings_of_the_same_nick_produce_different_uuids() {
    let _home = ScratchHome::new("uuid-casing");
    assert_ne!(offline_uuid("TestNick"), offline_uuid("testnick"));
}

#[test]
fn built_command_contains_both_system_properties_with_the_right_values() {
    let _home = ScratchHome::new("props");
    let merged = fake_merged("1.12.2-forge-test");
    let session = fake_session("TestNick", "the-real-token-value");
    let java = fake_java_path();
    let argv = build_launch_command(&session, 6, &merged, &java, true).unwrap();
    assert!(argv.contains(&"-Dcampfire.nick=TestNick".to_string()));
    assert!(argv.contains(&"-Dcampfire.token=the-real-token-value".to_string()));
}

#[test]
fn xmx_reflects_the_requested_ram() {
    let _home = ScratchHome::new("ram");
    let merged = fake_merged("1.12.2-forge-test-ram");
    let session = fake_session("TestNick", "tok");
    let java = fake_java_path();
    let argv = build_launch_command(&session, 8, &merged, &java, true).unwrap();
    assert!(argv.contains(&"-Xmx8G".to_string()));
    assert!(argv.contains(&"-Xms8G".to_string()));
}

#[test]
fn main_class_and_tweak_class_are_both_present() {
    let _home = ScratchHome::new("mainclass");
    let merged = fake_merged("1.12.2-forge-test-mc");
    let session = fake_session("TestNick", "tok");
    let java = fake_java_path();
    let argv = build_launch_command(&session, 6, &merged, &java, true).unwrap();
    assert!(argv.contains(&"net.minecraft.launchwrapper.Launch".to_string()));
    assert!(argv.iter().any(|a| a.contains("FMLTweaker")));
}

#[test]
fn every_classpath_entry_exists_on_disk_on_a_bootstrapped_directory() {
    let _home = ScratchHome::new("classpath");
    let merged = fake_merged("1.12.2-forge-test-cp");
    let session = fake_session("TestNick", "tok");
    let java = fake_java_path();
    let argv = build_launch_command(&session, 6, &merged, &java, true).unwrap();
    let cp_pos = argv.iter().position(|a| a == "-cp").unwrap();
    let cp = &argv[cp_pos + 1];
    for entry in std::env::split_paths(cp) {
        assert!(entry.is_file(), "classpath entry does not exist: {}", entry.display());
    }
}

#[test]
fn missing_classpath_entry_is_rejected_rather_than_silently_built() {
    let _home = ScratchHome::new("classpath-missing");
    let mut merged = fake_merged("1.12.2-forge-test-cp-missing");
    // Point the client jar at a path that was never written.
    merged.client_jar = campfire_launcher_core::paths::versions_dir().join("nope").join("nope.jar");
    let session = fake_session("TestNick", "tok");
    let java = fake_java_path();
    assert!(build_launch_command(&session, 6, &merged, &java, true).is_err());
}

#[test]
fn no_argument_contains_an_unsubstituted_placeholder() {
    let _home = ScratchHome::new("placeholders");
    let merged = fake_merged("1.12.2-forge-test-ph");
    let session = fake_session("TestNick", "tok");
    let java = fake_java_path();
    let argv = build_launch_command(&session, 6, &merged, &java, true).unwrap();
    assert!(!argv.iter().any(|a| a.contains("${")));
}

#[test]
fn the_logged_form_redacts_the_token_while_the_built_form_contains_it() {
    let _home = ScratchHome::new("redaction");
    let merged = fake_merged("1.12.2-forge-test-redact");
    let token = "super-secret-game-token-value";
    let session = fake_session("TestNick", token);
    let java = fake_java_path();
    let argv = build_launch_command(&session, 6, &merged, &java, true).unwrap();

    assert!(argv.iter().any(|a| a.contains(token)), "built argv must contain the real token");

    let log_contents = std::fs::read_to_string(log_path()).unwrap_or_default();
    assert!(!log_contents.contains(token), "log must never contain the real token");
    assert!(log_contents.contains("campfire.nick"), "log must still show the nick property");
}

#[test]
fn a_java_path_outside_the_runtime_directory_is_rejected() {
    let _home = ScratchHome::new("java-outside");
    let merged = fake_merged("1.12.2-forge-test-java");
    let session = fake_session("TestNick", "tok");
    let outside = std::env::temp_dir().join("not-under-runtime").join("java");
    write_dummy_file(&outside);
    assert!(build_launch_command(&session, 6, &merged, &outside, true).is_err());
}

#[test]
fn autoconnect_off_removes_only_the_two_trailing_arguments() {
    let _home = ScratchHome::new("autoconnect");
    let merged = fake_merged("1.12.2-forge-test-ac");
    let session = fake_session("TestNick", "tok");
    let java = fake_java_path();
    let with_ac = build_launch_command(&session, 6, &merged, &java, true).unwrap();
    let without_ac = build_launch_command(&session, 6, &merged, &java, false).unwrap();

    assert!(with_ac.contains(&"--server".to_string()));
    assert!(with_ac.contains(&"mc.campfire.pub".to_string()));
    assert!(!without_ac.contains(&"--server".to_string()));
    assert!(!without_ac.contains(&"mc.campfire.pub".to_string()));

    // Nothing else about the two vectors differs — the last four
    // (server/host/port/port-value) entries of `with_ac` are exactly the
    // difference, in the exact same order otherwise.
    assert_eq!(with_ac.len(), without_ac.len() + 4);
    assert_eq!(with_ac[..without_ac.len()], without_ac[..]);
}

#[test]
fn a_fresh_install_seeds_servers_dat_exactly_once() {
    let _home = ScratchHome::new("servers-dat");
    campfire_launcher_core::launch::seed_server_list();
    let dest = game_dir().join("servers.dat");
    assert!(dest.is_file());
    let seeded = std::fs::read(&dest).unwrap();
    assert!(!seeded.is_empty());

    // Overwrite with a marker byte, re-seed, and confirm it was left alone.
    std::fs::write(&dest, b"player-modified").unwrap();
    campfire_launcher_core::launch::seed_server_list();
    let after = std::fs::read(&dest).unwrap();
    assert_eq!(after, b"player-modified");
}
