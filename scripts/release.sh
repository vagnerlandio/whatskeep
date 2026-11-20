#!/bin/sh
# Prepares a release: sets the version everywhere, commits and creates the tag.
# Pushing the tag starts .github/workflows/release.yml (draft GitHub release).
#
#   scripts/release.sh 1.2.3
#   git push origin main v1.2.3
set -eu
cd "$(dirname "$0")/.."

version="${1:-}"
if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'; then
  echo "usage: scripts/release.sh <version>   (e.g. 1.2.3 or 1.3.0-beta.1)" >&2
  exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
  echo "the working tree has uncommitted changes; commit or stash them first" >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
  echo "tag v$version already exists" >&2
  exit 1
fi

# tauri.conf.json and the package.json files: the first "version" field.
for f in apps/desktop/src-tauri/tauri.conf.json apps/desktop/package.json packages/viewer/package.json packages/export-shell/package.json; do
  perl -0pi -e "s/\"version\": \"[^\"]*\"/\"version\": \"$version\"/" "$f"
done
# The app crate (the other crates are internal and keep their own versions).
perl -0pi -e "s/^version = \"[^\"]*\"/version = \"$version\"/m" apps/desktop/src-tauri/Cargo.toml
cargo update -p wa-historico --offline >/dev/null 2>&1 || cargo update -p wa-historico >/dev/null

git add apps/desktop/src-tauri/tauri.conf.json apps/desktop/src-tauri/Cargo.toml Cargo.lock \
  apps/desktop/package.json packages/viewer/package.json packages/export-shell/package.json
git commit -m "Release v$version"
git tag -a "v$version" -m "WhatsKeep v$version"
echo "Done. Now run: git push origin main v$version"
