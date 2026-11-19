#!/bin/sh
# Gera os pacotes de teste com o Rust e roda o e2e no navegador.
set -e
cd "$(dirname "$0")/.."
pnpm build
OUT="$(mktemp -d)"
ROOT="$(cd ../.. && pwd)"
WA_EXPORT_OUT="$OUT" WA_EXPORT_SHELL="$PWD/dist/index.html" WA_EXPORT_IMAGE="$PWD/e2e/foto.png" \
  cargo test --manifest-path "$ROOT/Cargo.toml" -p wa-export --test export -- --ignored gerar --quiet
node e2e/export.e2e.cjs "$OUT"
rm -rf "$OUT"
