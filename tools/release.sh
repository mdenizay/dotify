#!/bin/bash
# Builds Dotify, the dotify CLI and the dotify-mcp server, signs and notarizes
# them, publishes a GitHub release with auto-update metadata, and updates the
# cask and formula in mdenizay/homebrew-tap.
#
# Needs: `gh auth login`, a Developer ID Application certificate in the
# keychain, notary credentials stored with
#   xcrun notarytool store-credentials <profile> ...
# and the updater signing key at ~/.tauri/dotify.key (its public half is in
# apps/desktop/src-tauri/tauri.conf.json; losing it breaks auto-update).
#
# Bump the version in Cargo.toml, apps/desktop/package.json and
# apps/desktop/src-tauri/tauri.conf.json, add a "## <version>" section to
# CHANGELOG.md, commit, then run:
#   NOTARY_PROFILE=<profile> tools/release.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="mdenizay/dotify"
TAP="mdenizay/homebrew-tap"
PROFILE="${NOTARY_PROFILE:-whatsapp-zen}"
UPDATER_KEY="${TAURI_UPDATER_KEY:-$HOME/.tauri/dotify.key}"

VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' \
    "$ROOT/apps/desktop/src-tauri/tauri.conf.json")"
TAG="v$VERSION"
OUT="$ROOT/target/release-artifacts/$VERSION"
APP="$ROOT/target/release/bundle/macos/Dotify.app"
ZIP="$OUT/Dotify-$VERSION.zip"
UPDATE_TGZ="$OUT/Dotify-$VERSION.app.tar.gz"
CLI_TGZ="$OUT/dotify-cli-$VERSION-macos-arm64.tar.gz"

fail() { echo "error: $*" >&2; exit 1; }

# ── Preconditions ──────────────────────────────────────────────────────────
cd "$ROOT"
[[ -z "$(git status --porcelain)" ]] || fail "working tree is not clean"
[[ "$(git rev-parse --abbrev-ref HEAD)" == "main" ]] || fail "release from main"
git fetch -q origin main
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || fail "main is not in sync with origin/main"
! git rev-parse -q --verify "refs/tags/$TAG" >/dev/null || fail "tag $TAG already exists"
grep -q "^version = \"$VERSION\"" Cargo.toml || fail "Cargo.toml version does not match $VERSION"
grep -q "\"version\": \"$VERSION\"" apps/desktop/package.json || fail "package.json version does not match $VERSION"
[[ -f "$UPDATER_KEY" ]] || fail "updater signing key not found at $UPDATER_KEY"

NOTES="$(mktemp)"
trap 'rm -f "$NOTES"' EXIT
awk -v v="## $VERSION" '$0 == v {on = 1; next} /^## / {on = 0} on' CHANGELOG.md > "$NOTES"
[[ -s "$NOTES" ]] || fail "CHANGELOG.md has no '## $VERSION' section"

IDENTITY="${APPLE_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning \
    | awk -F'"' '/Developer ID Application/ {print $2; exit}')}"
[[ -n "$IDENTITY" ]] || fail "no Developer ID Application identity in the keychain"
xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null || fail "notary profile '$PROFILE' is not usable"

echo "Releasing Dotify $VERSION, signed by: $IDENTITY"
rm -rf "$OUT" && mkdir -p "$OUT"

# ── Build and sign ─────────────────────────────────────────────────────────
pnpm install --frozen-lockfile
APPLE_SIGNING_IDENTITY="$IDENTITY" pnpm --filter @dotify/desktop tauri build --bundles app
cargo build --release -p dotify-cli -p dotify-mcp

SIGNATURE="$(codesign -dvv "$APP" 2>&1 || true)"
[[ "$SIGNATURE" == *"Authority=Developer ID Application"* ]] \
    || fail "app is not Developer ID signed; Gatekeeper would block it"

CLI_DIR="$(mktemp -d)"
for BIN in dotify dotify-mcp; do
    cp "target/release/$BIN" "$CLI_DIR/$BIN"
    codesign --force --options runtime --timestamp --sign "$IDENTITY" "$CLI_DIR/$BIN"
done

# ── Notarize ───────────────────────────────────────────────────────────────
ditto -c -k --norsrc --noextattr --noqtn --keepParent "$APP" "$ZIP"
xcrun notarytool submit "$ZIP" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$APP"
rm -f "$ZIP"
ditto -c -k --norsrc --noextattr --noqtn --keepParent "$APP" "$ZIP"
spctl --assess --type execute "$APP"

# Command-line tools can't be stapled; notarize so Gatekeeper finds the ticket online.
(cd "$CLI_DIR" && zip -q "$CLI_DIR/cli.zip" dotify dotify-mcp)
xcrun notarytool submit "$CLI_DIR/cli.zip" --keychain-profile "$PROFILE" --wait
COPYFILE_DISABLE=1 tar --no-mac-metadata -czf "$CLI_TGZ" -C "$CLI_DIR" dotify dotify-mcp
rm -rf "$CLI_DIR"

# ── Auto-update artifacts (stapled app, signed with the updater key) ──────
COPYFILE_DISABLE=1 tar --no-mac-metadata -czf "$UPDATE_TGZ" -C "$(dirname "$APP")" Dotify.app
pnpm --filter @dotify/desktop tauri signer sign -f "$UPDATER_KEY" -p "" "$UPDATE_TGZ"
python3 - "$VERSION" "$NOTES" "$UPDATE_TGZ" "$OUT/latest.json" <<'PY'
import datetime, json, os, sys
version, notes, tgz, out = sys.argv[1:]
url = f"https://github.com/mdenizay/dotify/releases/download/v{version}/{os.path.basename(tgz)}"
signature = open(tgz + ".sig").read().strip()
json.dump({
    "version": version,
    "notes": open(notes).read().strip(),
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": {"darwin-aarch64": {"signature": signature, "url": url}},
}, open(out, "w"), indent=2)
PY

ZIP_SHA="$(shasum -a 256 "$ZIP" | cut -d' ' -f1)"
CLI_SHA="$(shasum -a 256 "$CLI_TGZ" | cut -d' ' -f1)"
(cd "$OUT" && shasum -a 256 ./*.zip ./*.tar.gz > SHA256SUMS)

# ── Publish ────────────────────────────────────────────────────────────────
git tag -a "$TAG" -m "Dotify $VERSION"
git push -q origin "$TAG"
gh release create "$TAG" --repo "$REPO" --title "Dotify $VERSION" --notes-file "$NOTES" \
    "$ZIP" "$UPDATE_TGZ" "$UPDATE_TGZ.sig" "$CLI_TGZ" "$OUT/latest.json" "$OUT/SHA256SUMS"

WORK="$(mktemp -d)"
gh repo clone "$TAP" "$WORK/tap" -- -q
mkdir -p "$WORK/tap/Casks" "$WORK/tap/Formula"
sed -e "s/VERSION/$VERSION/g" -e "s/SHA256/$ZIP_SHA/" packaging/dotify.rb | grep -v '^# ' > "$WORK/tap/Casks/dotify.rb"
sed -e "s/VERSION/$VERSION/g" -e "s/SHA256/$CLI_SHA/" packaging/dotify-cli.rb | grep -v '^# ' > "$WORK/tap/Formula/dotify-cli.rb"
git -C "$WORK/tap" add Casks/dotify.rb Formula/dotify-cli.rb
git -C "$WORK/tap" commit -q -m "dotify $VERSION"
git -C "$WORK/tap" push -q
rm -rf "$WORK"

echo "Released $TAG"
echo "  brew install --cask mdenizay/tap/dotify"
echo "  brew install mdenizay/tap/dotify-cli   # dotify and dotify-mcp"
