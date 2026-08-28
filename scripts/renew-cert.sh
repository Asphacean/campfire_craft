#!/usr/bin/env bash
# scripts/renew-cert.sh
#
# Creates the private CA once — never again, if ca/campfire-ca-key.pem
# already exists — and always re-issues the DOMAIN leaf, signed by that CA.
# ECDSA P-256 throughout (D-02): CA valid 3650 days, leaf valid 730 days.
# Re-run this roughly every two years to rotate the leaf; the root itself is
# generated exactly once, ever.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

log() { echo "[renew-cert] $*"; }

# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"
: "${DOMAIN:?DOMAIN not set in server.env}"

if [[ "$DOMAIN" == "mc.example.com" ]]; then
  echo "FATAL: DOMAIN is still the server.env.example placeholder (mc.example.com) — set the real domain in server.env before issuing a certificate" >&2
  exit 1
fi

CA_DIR="$ROOT_DIR/ca"
CA_KEY="$CA_DIR/campfire-ca-key.pem"
CA_CERT="$CA_DIR/campfire-ca.pem"
LEAF_KEY="$CA_DIR/$DOMAIN-key.pem"
LEAF_CERT="$CA_DIR/$DOMAIN-cert.pem"

if [[ ! -d "$CA_DIR" ]]; then
  mkdir -m 755 "$CA_DIR"
  log "created $CA_DIR"
fi

# mktemp, not a fixed /tmp path — cleaned up via EXIT trap regardless of how
# the script exits.
CSR_TMP=""
EXT_TMP=""
cleanup() {
  [[ -n "$CSR_TMP" ]] && rm -f "$CSR_TMP"
  [[ -n "$EXT_TMP" ]] && rm -f "$EXT_TMP"
}
trap cleanup EXIT

if [[ -f "$CA_KEY" ]]; then
  log "root CA already exists at $CA_KEY — left alone (the root is generated once, ever)"
else
  log "generating the root CA (once, ever) -> $CA_CERT"
  openssl ecparam -name prime256v1 -genkey -noout -out "$CA_KEY"
  openssl req -x509 -new -key "$CA_KEY" -sha256 -days 3650 \
    -subj "/CN=campfire.pub Root CA" -out "$CA_CERT" \
    -addext "basicConstraints=critical,CA:true" \
    -addext "keyUsage=critical,keyCertSign,cRLSign"
fi

log "issuing a fresh leaf for $DOMAIN"
CSR_TMP="$(mktemp)"
EXT_TMP="$(mktemp)"

openssl ecparam -name prime256v1 -genkey -noout -out "$LEAF_KEY"
openssl req -new -key "$LEAF_KEY" -subj "/CN=$DOMAIN" -out "$CSR_TMP"
cat >"$EXT_TMP" <<EOF
subjectAltName=DNS:$DOMAIN
extendedKeyUsage=serverAuth
basicConstraints=CA:false
keyUsage=digitalSignature,keyEncipherment
EOF
openssl x509 -req -in "$CSR_TMP" -CA "$CA_CERT" -CAkey "$CA_KEY" -CAcreateserial \
  -days 730 -sha256 -extfile "$EXT_TMP" -out "$LEAF_CERT"

if ! openssl verify -CAfile "$CA_CERT" "$LEAF_CERT" >/dev/null; then
  echo "FATAL: the freshly issued leaf does not verify against the CA" >&2
  exit 1
fi

# Set permissions explicitly — never rely on the umask.
chmod 600 "$CA_KEY"
chmod 644 "$CA_CERT" "$LEAF_CERT"
if getent group caddy >/dev/null 2>&1; then
  sudo chgrp caddy "$LEAF_KEY"
  chmod 640 "$LEAF_KEY"
else
  chmod 600 "$LEAF_KEY"
  log "NOTE: the 'caddy' group does not exist yet — leaf key left at 600; scripts/install-caddy.sh will fix ownership/mode once the caddy package creates that group"
fi

log "leaf issued and verified: $LEAF_CERT"
openssl x509 -in "$LEAF_CERT" -noout -enddate | sed 's/^notAfter=//'
