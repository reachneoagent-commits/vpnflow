#!/usr/bin/env bash
# Called by Tauri's Linux build hook from apps/desktop.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${TAURI_ENV_TARGET_TRIPLE:?Tauri must supply the package target}"
[[ $TARGET =~ ^[a-zA-Z0-9_]+-unknown-linux-(gnu|musl)(eabihf)?$ ]] || exit 2
cargo build --manifest-path "$ROOT/Cargo.toml" --locked --release --target "$TARGET" -p aegis-helper
install -D -m 0755 "$ROOT/target/$TARGET/release/aegis-helper" "$ROOT/target/linux-package/aegis-helper"
