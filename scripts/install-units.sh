#!/usr/bin/env bash
# Copies systemd/* into /etc/systemd/system and reloads the daemon. SUDO.
# Re-runnable; used by plan 01-02 (game unit) and plan 01-03 (backup units).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

shopt -s nullglob
UNITS=("$ROOT_DIR"/systemd/*)
shopt -u nullglob

if [[ ${#UNITS[@]} -eq 0 ]]; then
  echo "No unit files found in $ROOT_DIR/systemd — nothing to install." >&2
  exit 1
fi

for unit in "${UNITS[@]}"; do
  [[ -f "$unit" ]] || continue
  echo "Installing $(basename "$unit") -> /etc/systemd/system/"
  sudo install -m 644 "$unit" /etc/systemd/system/
done

sudo systemctl daemon-reload
echo "systemd units installed and daemon reloaded."
