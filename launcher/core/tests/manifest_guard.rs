//! The hostile-manifest suite (T-04-02-01/T-04-02-06): every rejection rule
//! proven against a crafted manifest, not asserted in a comment. No
//! network — every manifest here is built in memory and fed straight to
//! [`campfire_launcher_core::manifest::parse_manifest`] +
//! [`campfire_launcher_core::manifest::validate`].

use campfire_launcher_core::manifest::{parse_manifest, validate, SyncError};

fn scratch_game_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("campfire-manifest-guard-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn base_entry() -> serde_json::Value {
    serde_json::json!({
        "path": "mods/Example.jar",
        "sha256": "5468a9bef89c70784657ed54584768d61b76767db47b6131b67f28e0b253740",
        "size": 1234,
        "url": "mods/Example.jar"
    })
}

fn manifest_with(entries: Vec<serde_json::Value>, delete: Vec<&str>) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "pack_version": "2026-08-28T00:00:00Z",
        "mc": "1.12.2",
        "forge": "14.23.5.2860",
        "java": 8,
        "files": entries,
        "delete": delete,
    }))
    .unwrap()
}

fn assert_rejected(name: &str, entries: Vec<serde_json::Value>, delete: Vec<&str>) {
    let dir = scratch_game_dir(name);
    let bytes = manifest_with(entries, delete);
    let manifest = parse_manifest(&bytes).and_then(|m| validate(&m, &dir).map(|_| m));
    assert!(
        matches!(manifest, Err(SyncError::ManifestRejected(_))),
        "expected {name} to be rejected, got {manifest:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rejects_an_absolute_path() {
    let mut entry = base_entry();
    entry["path"] = serde_json::json!("/etc/campfire-owned");
    assert_rejected("absolute-path", vec![entry], vec![]);
}

#[test]
fn rejects_a_parent_directory_component_in_path() {
    let mut entry = base_entry();
    entry["path"] = serde_json::json!("mods/../../../etc/campfire-owned");
    assert_rejected("parent-in-path", vec![entry], vec![]);
}

#[test]
fn rejects_a_parent_directory_component_in_url() {
    let mut entry = base_entry();
    entry["url"] = serde_json::json!("mods/../../../etc/campfire-owned");
    assert_rejected("parent-in-url", vec![entry], vec![]);
}

#[test]
fn rejects_a_control_character() {
    let mut entry = base_entry();
    entry["path"] = serde_json::json!("mods/Example\u{0007}.jar");
    assert_rejected("control-char", vec![entry], vec![]);
}

#[test]
fn rejects_a_path_under_the_library_prefix() {
    let mut entry = base_entry();
    entry["path"] = serde_json::json!("libraries/net/minecraft/foo.jar");
    entry["url"] = serde_json::json!("libraries/net/minecraft/foo.jar");
    assert_rejected("library-prefix", vec![entry], vec![]);
}

#[test]
fn rejects_a_basename_that_looks_like_the_vanilla_client_jar() {
    let mut entry = base_entry();
    entry["path"] = serde_json::json!("mods/minecraft.jar");
    entry["url"] = serde_json::json!("mods/minecraft.jar");
    assert_rejected("client-jar-basename", vec![entry], vec![]);
}

#[test]
fn rejects_an_entry_missing_the_sha256_field() {
    let mut entry = base_entry();
    entry.as_object_mut().unwrap().remove("sha256");
    let dir = scratch_game_dir("missing-sha256");
    let bytes = manifest_with(vec![entry], vec![]);
    let result = parse_manifest(&bytes);
    assert!(matches!(result, Err(SyncError::ManifestRejected(_))), "expected a rejection, got {result:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rejects_a_delete_entry_with_a_parent_directory_component() {
    assert_rejected("delete-parent", vec![base_entry()], vec!["mods/../../../etc/campfire-owned"]);
}

#[test]
fn rejects_a_case_varied_never_touch_top_level_file() {
    let mut entry = base_entry();
    entry["path"] = serde_json::json!("OPTIONS.TXT");
    entry["url"] = serde_json::json!("OPTIONS.TXT");
    assert_rejected("case-varied-top-level-file", vec![entry], vec![]);
}

#[test]
fn rejects_a_case_varied_never_touch_directory() {
    let mut entry = base_entry();
    entry["path"] = serde_json::json!("Saves/World/level.dat");
    entry["url"] = serde_json::json!("Saves/World/level.dat");
    assert_rejected("case-varied-directory", vec![entry], vec![]);
}

#[test]
fn rejects_a_case_varied_never_touch_delete_entry() {
    assert_rejected("case-varied-delete", vec![base_entry()], vec!["Saves/World/level.dat"]);
}

#[test]
fn accepts_a_well_formed_manifest() {
    let dir = scratch_game_dir("accepted");
    let bytes = manifest_with(vec![base_entry()], vec!["mods/Removed.jar"]);
    let manifest = parse_manifest(&bytes).expect("valid manifest should parse");
    validate(&manifest, &dir).expect("well-formed manifest should be accepted");
    let _ = std::fs::remove_dir_all(&dir);
}
