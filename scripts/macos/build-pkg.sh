#!/usr/bin/env bash
# Build dist/VPNFlow-<version>.pkg: the app, the root daemon, wireguard-go and
# the launchd plist in one installer. Needs: Xcode command line tools, Go,
# Rust, pnpm. Unsigned unless SIGN_APP / SIGN_PKG identities are set.
#
#   scripts/macos/build-pkg.sh
#   ARCHS="arm64 x86_64" scripts/macos/build-pkg.sh        # universal
#   SIGN_PKG="Developer ID Installer: Name (TEAMID)" scripts/macos/build-pkg.sh
set -euo pipefail

WG_GO_TAG="${WG_GO_TAG:-0.0.20250522}"
WG_GO_URL="https://git.zx2c4.com/wireguard-go"
ARCHS="${ARCHS:-$(uname -m)}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="$ROOT/dist"
# Build outside synced folders: cloud drives re-add xattrs that pkgbuild turns into ._ files.
WORK="$(mktemp -d "${TMPDIR:-/tmp}/vpnflow-pkg.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)"
VERSION="${VERSION:-0.1.1}"

die() { echo "error: $*" >&2; exit 1; }
[[ $(uname) == Darwin ]] || die "macOS only"
for t in go cargo pnpm pkgbuild lipo; do command -v "$t" >/dev/null || die "missing tool: $t"; done


mkdir -p "$WORK/root/usr/local/libexec/vpnflow" "$WORK/root/Library/LaunchDaemons" \
	"$WORK/root/Applications" "$WORK/bin" "$OUT"

rust_triple() { [[ $1 == arm64 ]] && echo aarch64-apple-darwin || echo x86_64-apple-darwin; }
go_arch() { [[ $1 == arm64 ]] && echo arm64 || echo amd64; }

echo "==> wireguard-go $WG_GO_TAG (built from source)"
git clone --quiet --depth 1 --branch "$WG_GO_TAG" "$WG_GO_URL" "$WORK/wg-src"
echo "    commit: $(git -C "$WORK/wg-src" rev-parse HEAD)"
[[ -z ${WG_GO_COMMIT:-} || $(git -C "$WORK/wg-src" rev-parse HEAD) == "$WG_GO_COMMIT" ]] \
	|| die "wireguard-go commit does not match WG_GO_COMMIT"
for a in $ARCHS; do
	(cd "$WORK/wg-src" && CGO_ENABLED=0 GOOS=darwin GOARCH="$(go_arch "$a")" \
		go build -trimpath -ldflags="-s -w" -o "$WORK/bin/wireguard-go.$a" .)
done

echo "==> vpnflow-daemon"
for a in $ARCHS; do
	rustup target add "$(rust_triple "$a")" >/dev/null 2>&1 || true
	(cd "$ROOT" && cargo build --release --locked -p vpnflow-daemon --target "$(rust_triple "$a")")
	cp "$ROOT/target/$(rust_triple "$a")/release/vpnflow-daemon" "$WORK/bin/vpnflow-daemon.$a"
done

combine() { # name
	local files=() a
	for a in $ARCHS; do files+=("$WORK/bin/$1.$a"); done
	if [[ ${#files[@]} -gt 1 ]]; then lipo -create "${files[@]}" -output "$WORK/root/usr/local/libexec/vpnflow/$1"
	else cp "${files[0]}" "$WORK/root/usr/local/libexec/vpnflow/$1"; fi
}
combine wireguard-go
combine vpnflow-daemon
cp "$ROOT/packaging/macos/com.vpnflow.daemon.plist" "$WORK/root/Library/LaunchDaemons/"

echo "==> VPNFlow.app"
if [[ -z ${SKIP_APP:-} ]]; then
	targets=()
	for a in $ARCHS; do targets+=(--target "$(rust_triple "$a")"); done
	if [[ ${#targets[@]} -gt 2 ]]; then targets=(--target universal-apple-darwin); fi
	(cd "$ROOT/apps/desktop" && pnpm install --frozen-lockfile >/dev/null && pnpm tauri build --bundles app "${targets[@]}")
	app="$(find "$ROOT/target" -path '*/bundle/macos/VPNFlow.app' -maxdepth 8 -type d | head -1)"
	[[ -n $app ]] || die "VPNFlow.app was not built"
	ditto "$app" "$WORK/root/Applications/VPNFlow.app"
	[[ -z ${SIGN_APP:-} ]] || codesign --force --deep --options runtime --sign "$SIGN_APP" "$WORK/root/Applications/VPNFlow.app"
fi

if [[ -n ${SIGN_APP:-} ]]; then
	for f in wireguard-go vpnflow-daemon; do
		codesign --force --options runtime --sign "$SIGN_APP" "$WORK/root/usr/local/libexec/vpnflow/$f"
	done
fi

echo "==> package"
ditto --norsrc --noextattr --noqtn "$WORK/root" "$WORK/clean"
export COPYFILE_DISABLE=1
sign=()
[[ -z ${SIGN_PKG:-} ]] || sign=(--sign "$SIGN_PKG")
# Never let the installer "relocate" the app to an older copy it finds elsewhere
# (it picked the build output folder once and failed there).
pkgbuild --analyze --root "$WORK/clean" "$WORK/components.plist" >/dev/null
n="$(/usr/libexec/PlistBuddy -c 'Print' "$WORK/components.plist" | grep -c 'BundleIsRelocatable')"
for ((i = 0; i < n; i++)); do
	/usr/libexec/PlistBuddy -c "Set :$i:BundleIsRelocatable false" "$WORK/components.plist"
done
pkgbuild --root "$WORK/clean" --component-plist "$WORK/components.plist" --identifier com.vpnflow.pkg --version "$VERSION" \
	--ownership recommended --install-location / \
	--scripts "$ROOT/packaging/macos/scripts" ${sign[@]+"${sign[@]}"} "$OUT/VPNFlow-$VERSION.pkg"
shasum -a 256 "$OUT/VPNFlow-$VERSION.pkg" | tee "$OUT/VPNFlow-$VERSION.pkg.sha256"
echo "built $OUT/VPNFlow-$VERSION.pkg"
[[ -n ${SIGN_PKG:-} ]] || echo "note: unsigned; right-click > Open to install on this Mac"
