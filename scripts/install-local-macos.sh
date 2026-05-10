#!/usr/bin/env bash
set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This installer is only for macOS." >&2
  exit 1
fi

if ! command -v trash >/dev/null 2>&1; then
  echo "The 'trash' command is required so /Applications/Parley.app is not deleted directly." >&2
  exit 1
fi

IDENTITY_NAME="${PARLEY_CODESIGN_IDENTITY_NAME:-Parley Local Development}"
KEYCHAIN="${PARLEY_CODESIGN_KEYCHAIN:-$HOME/Library/Keychains/login.keychain-db}"
CERT_DIR="$HOME/Library/Application Support/com.vxncxnx.parley/dev-signing"
P12_PASS="${PARLEY_CODESIGN_P12_PASS:-parley-local}"

APP_TEMPLATE="$PWD/src-tauri/target/release/bundle/macos/Parley.app"
FRESH_BIN="$PWD/src-tauri/target/release/parley"
INSTALL_APP="/Applications/Parley.app"

find_identity_hash() {
  security find-identity -v -p codesigning "$KEYCHAIN" \
    | awk -v name="$IDENTITY_NAME" '$0 ~ name { print $2; exit }'
}

ensure_codesign_identity() {
  local identity_hash
  identity_hash="$(find_identity_hash)"
  if [[ -n "$identity_hash" ]]; then
    echo "$identity_hash"
    return
  fi

  mkdir -p "$CERT_DIR"

  cat > "$CERT_DIR/openssl.cnf" <<EOF
[ req ]
prompt = no
distinguished_name = dn
x509_extensions = v3_req

[ dn ]
CN = $IDENTITY_NAME

[ v3_req ]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
subjectKeyIdentifier = hash
EOF

  openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
    -keyout "$CERT_DIR/parley-local.key" \
    -out "$CERT_DIR/parley-local.crt" \
    -config "$CERT_DIR/openssl.cnf" \
    -sha256 >/dev/null 2>&1

  if [[ -f "$CERT_DIR/parley-local.p12" ]]; then
    trash "$CERT_DIR/parley-local.p12"
  fi

  openssl pkcs12 -legacy -export \
    -inkey "$CERT_DIR/parley-local.key" \
    -in "$CERT_DIR/parley-local.crt" \
    -out "$CERT_DIR/parley-local.p12" \
    -name "$IDENTITY_NAME" \
    -passout "pass:$P12_PASS" >/dev/null 2>&1

  security import "$CERT_DIR/parley-local.p12" \
    -k "$KEYCHAIN" \
    -P "$P12_PASS" \
    -T /usr/bin/codesign \
    -A >/dev/null

  security add-trusted-cert \
    -d \
    -r trustRoot \
    -p codeSign \
    -k "$KEYCHAIN" \
    "$CERT_DIR/parley-local.crt" >/dev/null

  identity_hash="$(find_identity_hash)"
  if [[ -z "$identity_hash" ]]; then
    echo "Failed to create a valid local code-signing identity." >&2
    exit 1
  fi

  echo "$identity_hash"
}

SIGN_IDENTITY="$(ensure_codesign_identity)"

bun tauri build --no-bundle

cp "$FRESH_BIN" "$APP_TEMPLATE/Contents/MacOS/parley"
chmod +x "$APP_TEMPLATE/Contents/MacOS/parley"
codesign --force --deep --sign "$SIGN_IDENTITY" "$APP_TEMPLATE"

pkill -f "$INSTALL_APP/Contents/MacOS/parley" 2>/dev/null || true

if [[ -d "$INSTALL_APP" ]]; then
  trash "$INSTALL_APP"
fi

cp -R "$APP_TEMPLATE" "$INSTALL_APP"
codesign --force --deep --sign "$SIGN_IDENTITY" "$INSTALL_APP"
codesign --verify --deep --strict "$INSTALL_APP"
codesign -dr - "$INSTALL_APP" 2>&1

open "$INSTALL_APP"
