#!/usr/bin/env bash
# scripts/release.sh — D-09's one-command release: makes the version
# true in tauri.conf.json (source of truth), Cargo.toml and Cargo.lock,
# commits, tags v<ver> annotated, and pushes — the one event
# .github/workflows/release.yml waits for. Building, signing, uploading
# and publishing the feed all belong to that pipeline, not this script.
#
# Refuses before touching anything: a version that isn't X.Y.Z, a dirty
# tree, a tag that already exists (local or origin), or a version lower
# than the one tauri.conf.json already carries (equal is allowed on
# purpose -- the first release cuts the version the file already has).
#
# Exit codes: 1 usage, 2 bad version format, 3 dirty tree,
# 4 tag already exists, 5 version too low, 6 cargo update failed.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TAURI_CONF="launcher/src-tauri/tauri.conf.json"
LAUNCHER_CARGO_TOML="launcher/Cargo.toml"
LAUNCHER_CARGO_LOCK="launcher/Cargo.lock"
CARGO_BIN="$HOME/.cargo/bin/cargo"

log() { echo "[release] $*" >&2; }

usage() {
  cat <<'EOF'
Usage: scripts/release.sh <X.Y.Z> [--no-push]
       scripts/release.sh --help

The single operator command that cuts a release: bumps the version in
launcher/src-tauri/tauri.conf.json (source of truth), launcher/Cargo.toml
and launcher/Cargo.lock, commits, creates annotated tag v<X.Y.Z>, and
pushes the current branch with tags to origin -- the one event
.github/workflows/release.yml waits for.

  --no-push   Do everything except the final push (rehearsable in a
              throwaway clone).
  --help      Show this message and exit.
EOF
}

NO_PUSH=false
VERSION=""
while [ $# -gt 0 ]; do
  case "$1" in
    --no-push) NO_PUSH=true; shift ;;
    --help) usage; exit 0 ;;
    -*) echo "FATAL: unknown argument: $1" >&2; usage >&2; exit 1 ;;
    *)
      if [ -n "$VERSION" ]; then
        echo "FATAL: unexpected extra argument: $1" >&2
        usage >&2
        exit 1
      fi
      VERSION="$1"
      shift
      ;;
  esac
done

if [ -z "$VERSION" ]; then
  echo "FATAL: a version argument is required" >&2
  usage >&2
  exit 1
fi

if ! [[ "$VERSION" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "FATAL: '$VERSION' is not major.minor.patch (three dot-separated integers, no leading zeros -- a leading zero like '08' is invalid octal in bash's arithmetic comparison below and would crash is_lower instead of being cleanly rejected here)" >&2
  exit 2
fi
NEW_MAJOR="${BASH_REMATCH[1]}"
NEW_MINOR="${BASH_REMATCH[2]}"
NEW_PATCH="${BASH_REMATCH[3]}"

if [ -n "$(git status --porcelain)" ]; then
  echo "FATAL: working tree is dirty -- commit or stash before cutting a release" >&2
  exit 3
fi

TAG="v$VERSION"
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  echo "FATAL: tag $TAG already exists locally" >&2
  exit 4
fi
if git remote get-url origin >/dev/null 2>&1; then
  if git ls-remote --exit-code --tags origin "refs/tags/$TAG" >/dev/null 2>&1; then
    echo "FATAL: tag $TAG already exists on origin" >&2
    exit 4
  fi
else
  log "no 'origin' remote configured -- skipping the remote tag check (rehearsal clone?)"
fi

CURRENT_VERSION="$(jq -r .version "$TAURI_CONF")"
if ! [[ "$CURRENT_VERSION" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "FATAL: current version '$CURRENT_VERSION' in $TAURI_CONF is not major.minor.patch -- can't compare" >&2
  exit 2
fi
CUR_MAJOR="${BASH_REMATCH[1]}"
CUR_MINOR="${BASH_REMATCH[2]}"
CUR_PATCH="${BASH_REMATCH[3]}"

# Numeric (major, minor, patch) comparison -- a plain string compare gets
# "0.9.0" vs "0.10.0" backwards, the same rule
# launcher/core/src/update.rs's own version comparison follows.
is_lower() {
  local a_major=$1 a_minor=$2 a_patch=$3 b_major=$4 b_minor=$5 b_patch=$6
  if [ "$a_major" -lt "$b_major" ]; then return 0; fi
  if [ "$a_major" -gt "$b_major" ]; then return 1; fi
  if [ "$a_minor" -lt "$b_minor" ]; then return 0; fi
  if [ "$a_minor" -gt "$b_minor" ]; then return 1; fi
  [ "$a_patch" -lt "$b_patch" ]
}

if is_lower "$NEW_MAJOR" "$NEW_MINOR" "$NEW_PATCH" "$CUR_MAJOR" "$CUR_MINOR" "$CUR_PATCH"; then
  echo "FATAL: $VERSION is lower than the current version $CURRENT_VERSION in $TAURI_CONF" >&2
  exit 5
fi

log "Cutting release $TAG (current: $CURRENT_VERSION)"

# --- Act: make the version true everywhere it's stated ---------------------

# tauri.conf.json: jq into a temp file in the same directory, then a
# rename -- an interrupted run never leaves a half-written config, only
# either the old file or the new one, never a truncated one.
TMP_CONF="$(mktemp "${TAURI_CONF}.XXXXXX")"
jq --arg v "$VERSION" '.version = $v' "$TAURI_CONF" > "$TMP_CONF"
mv "$TMP_CONF" "$TAURI_CONF"
log "Updated $TAURI_CONF -> $VERSION"

# launcher/Cargo.toml: only the version line inside [workspace.package],
# never a version string a dependency table might also carry.
TMP_TOML="$(mktemp "${LAUNCHER_CARGO_TOML}.XXXXXX")"
awk -v ver="$VERSION" '
  /^\[workspace\.package\]/ { in_ws_pkg = 1; print; next }
  /^\[/ { in_ws_pkg = 0 }
  in_ws_pkg && !done && /^version = "/ {
    print "version = \"" ver "\""
    done = 1
    next
  }
  { print }
  END { if (!done) { print "FATAL: no [workspace.package] version line found" > "/dev/stderr"; exit 1 } }
' "$LAUNCHER_CARGO_TOML" > "$TMP_TOML"
mv "$TMP_TOML" "$LAUNCHER_CARGO_TOML"
log "Updated $LAUNCHER_CARGO_TOML -> $VERSION"

# launcher/Cargo.lock: refresh so the workspace members' own recorded
# versions follow -- offline first (the common case, nothing new to
# fetch), falling back to a networked run only if the offline one
# refuses. Run from inside launcher/ so the rustup shim resolves
# rust-toolchain.toml's 1.98.0 pin (a toolchain override is resolved
# from the current directory, not --manifest-path).
if ! (cd launcher && "$CARGO_BIN" update --workspace --offline) 2>/tmp/release-cargo-update.log; then
  log "offline cargo update refused, retrying with network access"
  if ! (cd launcher && "$CARGO_BIN" update --workspace) 2>>/tmp/release-cargo-update.log; then
    cat /tmp/release-cargo-update.log >&2
    rm -f /tmp/release-cargo-update.log
    echo "FATAL: cargo update failed both offline and online -- see output above" >&2
    exit 6
  fi
fi
rm -f /tmp/release-cargo-update.log
log "Refreshed $LAUNCHER_CARGO_LOCK"

git add "$TAURI_CONF" "$LAUNCHER_CARGO_TOML" "$LAUNCHER_CARGO_LOCK"
git commit -q -m "release: v$VERSION"
git tag -a "$TAG" -m "Release $VERSION"
log "Committed and tagged $TAG"

if [ "$NO_PUSH" = true ]; then
  log "no-push: stopping here. Run 'git push origin HEAD --follow-tags' when ready."
  exit 0
fi

CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
git push origin "$CURRENT_BRANCH" --follow-tags
log "Pushed $TAG -- watch the build at https://github.com/Asphacean/campfire_craft/actions"
