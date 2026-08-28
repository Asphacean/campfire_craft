#!/usr/bin/env python3
"""scripts/assemble-client.py — the client half of the manifest contract,
run as an automated proof of manifest completeness and correctness.

Downloads (or re-verifies) a full client directory built purely from
pack/manifest.json served over HTTPS, trusting only our own CA. This is the
reference implementation Phase 4's launcher mirrors: build an SSL context
from the CA file only (no system-trust-store fallback, no insecure escape
hatch), validate every manifest path before touching disk, hash-verify every
downloaded byte before it is trusted, and honour delete[].

Standard library only.

Usage:
  assemble-client.py [--base-url URL] [--cacert PATH] [--dest DIR] [--verify]

Exit codes:
  0 = every entry verified (ASSEMBLE OK / VERIFY OK)
  1 = usage / manifest-fetch error
  2 = the manifest itself failed the client-side path guard or DIST-03 gate
  3 = one or more files failed to download or hash-verify
"""
import argparse
import hashlib
import json
import os
import ssl
import sys
import tempfile
import urllib.parse
import urllib.request

CHUNK_SIZE = 1 << 20  # 1 MiB streaming read
PROGRESS_EVERY = 100

# DIST-03: our host never serves the Minecraft client jar, libraries or
# assets — those come from Mojang. A manifest path under any of these
# prefixes, or a basename that looks like the vanilla client jar, is a hard
# failure of the run.
FORBIDDEN_PREFIXES = ("libraries/", "assets/", "versions/")


def log(msg: str) -> None:
    print(f"[assemble-client] {msg}")


def parse_server_env() -> dict:
    """Best-effort read of server.env for the --base-url default. Not a
    hard dependency — falls back to the hardcoded default if unreadable."""
    env: dict[str, str] = {}
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    env_path = os.path.join(repo_root, "server.env")
    try:
        with open(env_path, "r") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#") or "=" not in line:
                    continue
                key, _, val = line.partition("=")
                env[key.strip()] = val.strip().strip('"')
    except OSError:
        pass
    return env


def default_base_url() -> str:
    env = parse_server_env()
    domain = env.get("DOMAIN", "mc.campfire.pub")
    port = env.get("HTTPS_PORT", "8444")
    return f"https://{domain}:{port}"


def default_cacert() -> str:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    return os.path.join(repo_root, "ca", "campfire-ca.pem")


def default_dest() -> str:
    return os.path.expanduser("~/client-check")


def build_ssl_context(cacert: str) -> ssl.SSLContext:
    if not os.path.isfile(cacert):
        log(f"FATAL: CA certificate not found: {cacert}")
        sys.exit(1)
    return ssl.create_default_context(cafile=cacert)


def fetch_manifest(base_url: str, ctx: ssl.SSLContext) -> dict:
    url = f"{base_url}/manifest.json"
    try:
        with urllib.request.urlopen(url, context=ctx, timeout=30) as resp:
            body = resp.read()
    except Exception as e:
        log(f"FATAL: could not fetch manifest from {url}: {e}")
        sys.exit(1)
    try:
        return json.loads(body)
    except json.JSONDecodeError as e:
        log(f"FATAL: manifest at {url} is not valid JSON: {e}")
        sys.exit(1)


def looks_like_minecraft_client_jar(basename: str) -> bool:
    lowered = basename.lower()
    return lowered.startswith("minecraft") and lowered.endswith(".jar")


def validate_manifest_entries(manifest: dict, dest: str) -> list[dict]:
    """The client-side path guard (T-03-02-08): reject the whole run if any
    path/url is absolute, contains a '..' component, contains a control
    character, or resolves outside dest once joined. Also the DIST-03
    assertion as a first-class check, not a comment."""
    real_dest = os.path.realpath(dest)
    files = manifest.get("files", [])
    forbidden_hits = []
    for entry in files:
        for field in ("path", "url"):
            value = entry.get(field, "")
            if os.path.isabs(value):
                log(f"FATAL: manifest {field} is absolute: {value}")
                sys.exit(2)
            if ".." in value.split("/"):
                log(f"FATAL: manifest {field} contains a '..' component: {value}")
                sys.exit(2)
            if any(ord(c) < 0x20 or ord(c) == 0x7F for c in value):
                log(f"FATAL: manifest {field} contains a control character: {value}")
                sys.exit(2)
            real_path = os.path.realpath(os.path.join(dest, value))
            if os.path.commonpath([real_dest, real_path]) != real_dest:
                log(f"FATAL: manifest {field} resolves outside the destination: {value}")
                sys.exit(2)
        path = entry["path"]
        basename = os.path.basename(path)
        if path.startswith(FORBIDDEN_PREFIXES) or looks_like_minecraft_client_jar(basename):
            forbidden_hits.append(path)

    if forbidden_hits:
        log("FATAL: DIST-03 violated — the manifest references Minecraft client jar/library/asset paths:")
        for h in forbidden_hits:
            log(f"  - {h}")
        sys.exit(2)

    for path in manifest.get("delete", []):
        if os.path.isabs(path) or ".." in path.split("/"):
            log(f"FATAL: delete[] entry fails the path guard: {path}")
            sys.exit(2)

    return files


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            chunk = f.read(CHUNK_SIZE)
            if not chunk:
                break
            h.update(chunk)
    return h.hexdigest()


def download_entry(base_url: str, ctx: ssl.SSLContext, entry: dict, dest: str) -> tuple[bool, str]:
    path = entry["path"]
    url_part = entry["url"]
    expected_sha = entry["sha256"]
    expected_size = entry["size"]
    dest_path = os.path.join(dest, path)
    os.makedirs(os.path.dirname(dest_path), exist_ok=True)

    if os.path.isfile(dest_path) and os.path.getsize(dest_path) == expected_size:
        if sha256_file(dest_path) == expected_sha:
            return True, "already present, hash matches"

    # quote() with the default safe='/' percent-encodes spaces and other
    # reserved characters in each path segment while leaving the '/'
    # separators alone — several pack filenames genuinely contain spaces
    # (e.g. "resources/mainmenu/images/4 new.jpg"), which urllib.request
    # otherwise rejects outright ("URL can't contain control characters").
    url = f"{base_url}/pack/{urllib.parse.quote(url_part)}"
    tmp_fd, tmp_path = tempfile.mkstemp(dir=os.path.dirname(dest_path))
    try:
        with os.fdopen(tmp_fd, "wb") as tmp_f:
            try:
                with urllib.request.urlopen(url, context=ctx, timeout=60) as resp:
                    while True:
                        chunk = resp.read(CHUNK_SIZE)
                        if not chunk:
                            break
                        tmp_f.write(chunk)
            except Exception as e:
                return False, f"download failed: {e}"
        actual_size = os.path.getsize(tmp_path)
        actual_sha = sha256_file(tmp_path)
        if actual_size != expected_size or actual_sha != expected_sha:
            return False, (
                f"hash/size mismatch — expected sha256={expected_sha} size={expected_size}, "
                f"got sha256={actual_sha} size={actual_size}"
            )
        os.replace(tmp_path, dest_path)
        return True, "downloaded and verified"
    finally:
        if os.path.exists(tmp_path):
            os.unlink(tmp_path)


def verify_entry(entry: dict, dest: str) -> tuple[bool, str]:
    path = entry["path"]
    expected_sha = entry["sha256"]
    expected_size = entry["size"]
    dest_path = os.path.join(dest, path)
    if not os.path.isfile(dest_path):
        return False, "missing"
    actual_size = os.path.getsize(dest_path)
    if actual_size != expected_size:
        return False, f"size mismatch — expected {expected_size}, got {actual_size}"
    actual_sha = sha256_file(dest_path)
    if actual_sha != expected_sha:
        return False, (
            f"hash mismatch — expected sha256={expected_sha}, got sha256={actual_sha} "
            f"(size {actual_size} bytes, expected {expected_size})"
        )
    return True, "verified"


def managed_dirs(files: list[dict]) -> set[str]:
    dirs = set()
    for entry in files:
        top = entry["path"].split("/", 1)[0]
        dirs.add(top)
    return dirs


def find_unmanaged(dest: str, files: list[dict]) -> list[str]:
    manifest_paths = {f["path"] for f in files}
    dirs = managed_dirs(files)
    unmanaged = []
    for top in sorted(dirs):
        top_dir = os.path.join(dest, top)
        if not os.path.isdir(top_dir):
            continue
        for dirpath, _, filenames in os.walk(top_dir):
            for name in filenames:
                full = os.path.join(dirpath, name)
                rel = os.path.relpath(full, dest).replace(os.sep, "/")
                if rel not in manifest_paths:
                    unmanaged.append(rel)
    return sorted(unmanaged)


def apply_deletes(manifest: dict, dest: str) -> None:
    for path in manifest.get("delete", []):
        target = os.path.join(dest, path)
        if os.path.isfile(target):
            os.remove(target)
            log(f"deleted (per manifest delete[]): {path}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default=None, help="HTTPS front base URL (default: from server.env DOMAIN/HTTPS_PORT)")
    parser.add_argument("--cacert", default=default_cacert(), help="Path to the pinned CA cert (default: repo's ca/campfire-ca.pem)")
    parser.add_argument("--dest", default=default_dest(), help="Destination directory (default: ~/client-check)")
    parser.add_argument("--verify", action="store_true", help="Re-check an existing destination without downloading")
    args = parser.parse_args()

    base_url = args.base_url or default_base_url()
    dest = os.path.abspath(args.dest)
    os.makedirs(dest, exist_ok=True)

    ctx = build_ssl_context(args.cacert)
    manifest = fetch_manifest(base_url, ctx)
    files = validate_manifest_entries(manifest, dest)

    total = len(files)
    total_bytes = sum(f["size"] for f in files)
    failures = []

    if args.verify:
        apply_deletes(manifest, dest)
        for i, entry in enumerate(files, 1):
            ok, detail = verify_entry(entry, dest)
            if not ok:
                failures.append((entry["path"], detail))
            if i % PROGRESS_EVERY == 0:
                log(f"...{i}/{total} verified")
        unmanaged = find_unmanaged(dest, files)
        if unmanaged:
            log(f"NOTE: {len(unmanaged)} file(s) present in managed directories but not listed in the manifest:")
            for u in unmanaged:
                log(f"  - {u}")
    else:
        apply_deletes(manifest, dest)
        for i, entry in enumerate(files, 1):
            ok, detail = download_entry(base_url, ctx, entry, dest)
            if not ok:
                failures.append((entry["path"], detail))
            if i % PROGRESS_EVERY == 0:
                log(f"...{i}/{total} processed")

    if failures:
        log(f"FAILED: {len(failures)}/{total} entries did not verify:")
        for path, detail in failures:
            log(f"  - {path}: {detail}")
        return 3

    verdict = "VERIFY OK" if args.verify else "ASSEMBLE OK"
    log(f"{verdict} — {total} files, {total_bytes} bytes, dest={dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
