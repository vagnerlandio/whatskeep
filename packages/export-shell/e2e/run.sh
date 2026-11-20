#!/bin/sh
# Generates the test packages with Rust and runs the e2e in the browser.
set -e
cd "$(dirname "$0")/.."
pnpm build
OUT="$(mktemp -d)"
ROOT="$(cd ../.. && pwd)"
WK_EXPORT_OUT="$OUT" WK_EXPORT_SHELL="$PWD/dist/index.html" WK_EXPORT_IMAGE="$PWD/e2e/foto.png" \
  cargo test --manifest-path "$ROOT/Cargo.toml" -p wk-export --test export -- --ignored generate_browser_packages --quiet
node e2e/export.e2e.cjs "$OUT"
rm -rf "$OUT"
