#!/usr/bin/env bash
# Builds the interactive website demo into docs/embed: the real desktop interface
# running against a mock backend (apps/desktop/src/demo/tauri-mock.ts).
set -euo pipefail
cd "$(dirname "$0")/../apps/desktop"

pnpm exec vite build --config vite.demo.config.ts

index=../../docs/embed/index.html
# The demo runs inside the landing page hero, so keep search engines on that page.
sed -i 's|<meta charset="UTF-8" />|&<meta name="robots" content="noindex" />|' "$index"
grep -q 'name="robots"' "$index"
