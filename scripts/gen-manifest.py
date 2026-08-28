#!/usr/bin/env python3
"""scripts/gen-manifest.py — the last step of scripts/publish-pack.sh.

Walks the managed content of a pack root, hashes it, validates every path,
diffs it against the previous manifest for a cumulative delete[] list, and
swaps a fresh manifest.json in atomically. Standard library only (D-08's
locked schema; no third-party import may appear in this file).

Usage: gen-manifest.py <pack_root>

Exit codes:
  0 = manifest written
  1 = usage error
  2 = a collected path failed traversal validation (aborts before writing)
  3 = the forbidden-content gate fired (aborts before writing)
"""
import hashlib
import json
import os
import sys
import tempfile
from datetime import datetime, timezone

# Never managed, never in files[] and never in delete[] (D-08).
NEVER_MANAGED_DIRS = {"saves", "screenshots", "logs", "crash-reports"}
NEVER_MANAGED_FILES = {"options.txt", "optionsof.txt", "servers.dat"}

# Forbidden-content gate (T-03-02-02): these must never be published to the
# internet, no matter what directory they end up in.
FORBIDDEN_BASENAMES = {
    "server.properties",
    "ops.json",
    "whitelist.json",
    "usercache.json",
    "server.env",
    "eula.txt",
}
FORBIDDEN_PREFIX = "banned-"
FORBIDDEN_SUFFIX = ".db"
FORBIDDEN_COMPONENT = "saves"

CHUNK_SIZE = 1 << 20  # 1 MiB streaming read


def log(msg: str) -> None:
    print(f"[gen-manifest] {msg}")


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            chunk = f.read(CHUNK_SIZE)
            if not chunk:
                break
            h.update(chunk)
    return h.hexdigest()


def collect_paths(pack_root: str) -> list[str]:
    """Walk pack_root, returning sorted relative paths of every managed
    regular file. Excludes manifest.json itself, dotfiles/dotdirectories,
    and the never-managed set."""
    collected: list[str] = []
    for dirpath, dirnames, filenames in os.walk(pack_root):
        # Prune dot-directories and never-managed directories in place so
        # os.walk never descends into them.
        dirnames[:] = [
            d for d in dirnames
            if not d.startswith(".") and d not in NEVER_MANAGED_DIRS
        ]
        for name in filenames:
            if name.startswith("."):
                continue
            if name in NEVER_MANAGED_FILES:
                continue
            full = os.path.join(dirpath, name)
            if not os.path.isfile(full) or os.path.islink(full):
                continue
            rel = os.path.relpath(full, pack_root)
            if rel == "manifest.json":
                continue
            collected.append(rel)
    # Explicit sort: os.walk's order is not guaranteed, and an unsorted list
    # makes two identical publishes produce differently-ordered manifests.
    collected.sort()
    return collected


def validate_paths(pack_root: str, rel_paths: list[str]) -> None:
    """Hard error, aborting the whole run, on any path that could escape
    pack_root once resolved. The write side of the traversal control
    (T-03-02-01/T-03-02-08) — a bad entry must never be published at all."""
    real_root = os.path.realpath(pack_root)
    for rel in rel_paths:
        if os.path.isabs(rel):
            log(f"FATAL: collected path is absolute: {rel}")
            sys.exit(2)
        if ".." in rel.split(os.sep):
            log(f"FATAL: collected path contains a '..' component: {rel}")
            sys.exit(2)
        if any(ord(c) < 0x20 or ord(c) == 0x7F or c == "\x00" for c in rel):
            log(f"FATAL: collected path contains a control/null character: {rel}")
            sys.exit(2)
        real_path = os.path.realpath(os.path.join(pack_root, rel))
        if os.path.commonpath([real_root, real_path]) != real_root:
            log(f"FATAL: collected path resolves outside the pack root: {rel}")
            sys.exit(2)


def apply_forbidden_content_gate(rel_paths: list[str]) -> None:
    """Abort the run, non-zero, naming every offender (T-03-02-02). This
    gate lives before the manifest that would advertise them is ever
    written — a static file server has no second chance."""
    offenders = []
    for rel in rel_paths:
        basename = os.path.basename(rel)
        components = rel.split(os.sep)
        if (
            basename in FORBIDDEN_BASENAMES
            or basename.startswith(FORBIDDEN_PREFIX)
            or basename.endswith(FORBIDDEN_SUFFIX)
            or FORBIDDEN_COMPONENT in components
        ):
            offenders.append(rel)
    if offenders:
        log("FATAL: forbidden-content gate fired — the following paths must never be published:")
        for o in offenders:
            log(f"  - {o}")
        sys.exit(3)


def build_files(pack_root: str, rel_paths: list[str]) -> list[dict]:
    entries = []
    for rel in rel_paths:
        full = os.path.join(pack_root, rel)
        size = os.stat(full).st_size
        # Zero-byte files exist for real in server/config/ (mod-author
        # placeholder markers, e.g. "Put biome config files here") — a
        # 0-byte entry has nothing to hash-verify and would violate the
        # manifest's own size>0 invariant every consumer (assemble-client.py,
        # Phase 4's launcher) relies on. Excluded here, from files[] only —
        # they were already collected and gated above (the forbidden-content
        # gate must see every file regardless of size), and they stay on
        # disk unchanged; only the manifest omits them.
        if size == 0:
            continue
        url = rel.replace(os.sep, "/")
        entries.append({
            "path": url,
            "sha256": sha256_file(full),
            "size": size,
            "url": url,
        })
    return entries


def load_previous_manifest(manifest_path: str) -> dict | None:
    if not os.path.isfile(manifest_path):
        return None
    try:
        with open(manifest_path, "r") as f:
            return json.load(f)
    except (OSError, json.JSONDecodeError) as e:
        log(f"Previous manifest at {manifest_path} unreadable ({e}) — starting with an empty delete list")
        return None


def compute_delete_list(previous: dict | None, current_paths: set[str]) -> list[str]:
    """The union of two sets, sorted:
      - the previous manifest's own delete[] entries still absent from the
        current file list (carried forward — a client several publishes
        behind still learns about a file removed two publishes ago)
      - paths that were in the previous files[] and are absent now
    """
    if previous is None:
        return []
    prev_delete = set(previous.get("delete", []))
    prev_files = {f["path"] for f in previous.get("files", [])}
    newly_removed = prev_files - current_paths
    carried_forward = prev_delete - current_paths
    return sorted(carried_forward | newly_removed)


def write_manifest_atomic(manifest: dict, dest_path: str) -> None:
    dest_dir = os.path.dirname(dest_path)
    fd, tmp_path = tempfile.mkstemp(dir=dest_dir, suffix=".tmp")
    try:
        with os.fdopen(fd, "w") as f:
            json.dump(manifest, f, indent=2, sort_keys=True)
        os.replace(tmp_path, dest_path)  # atomic on the same filesystem
    except Exception:
        if os.path.exists(tmp_path):
            os.unlink(tmp_path)
        raise
    os.chmod(dest_path, 0o644)


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 1
    pack_root = os.path.abspath(sys.argv[1])
    if not os.path.isdir(pack_root):
        log(f"FATAL: pack root does not exist: {pack_root}")
        return 1

    manifest_path = os.path.join(pack_root, "manifest.json")

    rel_paths = collect_paths(pack_root)
    validate_paths(pack_root, rel_paths)
    apply_forbidden_content_gate(rel_paths)

    previous = load_previous_manifest(manifest_path)
    files = build_files(pack_root, rel_paths)
    current_paths = {f["path"] for f in files}
    delete = compute_delete_list(previous, current_paths)

    manifest = {
        "pack_version": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "mc": "1.12.2",
        "forge": "14.23.5.2860",
        "java": 8,
        "files": files,
        "delete": delete,
    }

    write_manifest_atomic(manifest, manifest_path)

    total_bytes = sum(f["size"] for f in files)
    log(
        f"files={len(files)} delete={len(delete)} "
        f"total_bytes={total_bytes} pack_version={manifest['pack_version']}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
