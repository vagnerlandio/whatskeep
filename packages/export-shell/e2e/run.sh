#!/bin/sh
# Generates the test packages with Rust and runs the e2e in the browser.
set -e
cd "$(dirname "$0")/.."
pnpm build
OUT="$(mktemp -d)"
ROOT="$(cd ../.. && pwd)"
WA_EXPORT_OUT="$OUT" WA_EXPORT_SHELL="$PWD/dist/index.html" WA_EXPORT_IMAGE="$PWD/e2e/foto.png" \
  cargo test --manifest-path "$ROOT/Cargo.toml" -p wa-export --test export -- --ignored generate_browser_packages --quiet
node e2e/export.e2e.cjs "$OUT"
rm -rf "$OUT"
