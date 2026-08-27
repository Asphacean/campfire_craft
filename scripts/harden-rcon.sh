#!/usr/bin/env bash
# Scoped nftables table that drops non-loopback traffic to the RCON port
# (D-08). Minecraft 1.12.2 has no property to bind RCON independently of the
# game port, so the intent is enforced at the host firewall instead.
#
# Deliberately a dedicated table with `policy accept` and exactly two rules —
# never a default-drop ruleset — so this cannot lock the operator out of SSH
# or affect any other service on this Pi. SUDO.
#
# [Rule 1 - Bug, deviation from 01-02-PLAN.md] The plan's literal path is
# `systemctl enable --now nftables`, loading Debian's package-default
# /etc/nftables.conf (which starts with `flush ruleset`). This Pi runs Docker
# (PROJECT.md), which manages its own NAT/forward rules via iptables-nft in a
# separate `table ip filter` — a global `flush ruleset` on every nftables.service
# (re)start would wipe those out, breaking container networking, directly
# violating this very task's "cannot ... affect any other service on this Pi"
# requirement. Fixed by loading only this script's own scoped `table inet
# rlcraft` via a dedicated oneshot unit (rlcraft-nft.service), never touching
# /etc/nftables.conf or nftables.service. See 01-02-SUMMARY.md Deviations.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"
RCON_PORT="${RCON_PORT:-25575}"

NFT_DIR="/etc/nftables.d"
NFT_RULE_FILE="$NFT_DIR/rlcraft-rcon.nft"

echo "== Writing $NFT_RULE_FILE =="
sudo mkdir -p "$NFT_DIR"
sudo tee "$NFT_RULE_FILE" > /dev/null <<EOF
#!/usr/sbin/nft -f
# Managed by scripts/harden-rcon.sh — dedicated table, policy accept, exactly
# two rules. Do not add a default-drop policy here or anywhere else on this
# host; that is explicitly out of scope (T-02-08). Loaded standalone by
# rlcraft-nft.service, NOT via /etc/nftables.conf (see script header: Debian's
# default nftables.conf does a full ruleset flush, which would wipe Docker's
# iptables-nft-managed rules on this host).
#
# The empty declare-then-delete pair makes re-runs idempotent: on a first run
# the table doesn't exist yet, so "delete" alone would error; declaring an
# empty table first guarantees it exists before the delete, then it's
# redefined fresh below. This only ever touches "table inet rlcraft" — never
# the global ruleset — so Docker's "table ip filter" is untouched.
table inet rlcraft {}
delete table inet rlcraft

table inet rlcraft {
	chain rcon_input {
		type filter hook input priority -10; policy accept;
		iif "lo" accept
		tcp dport ${RCON_PORT} drop
	}
}
EOF

echo "== Loading ruleset now =="
sudo nft -f "$NFT_RULE_FILE"

echo "== Installing rlcraft-nft.service (boot-time loader, not nftables.service) =="
sudo install -m 644 "$ROOT_DIR/systemd/rlcraft-nft.service" /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now rlcraft-nft.service

echo "== RCON listener bind address (residual) =="
ss -tlnp 2>/dev/null | grep ":${RCON_PORT} " || echo "  (no listener found on :${RCON_PORT} — is rlcraft running?)"

echo "== Verifying =="
sudo nft list table inet rlcraft
echo "== No default-drop policy anywhere =="
sudo nft list ruleset | grep -c 'policy drop' || true
