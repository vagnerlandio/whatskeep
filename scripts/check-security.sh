#!/bin/sh
# Blocks dangerous patterns in the frontend code. Messages are third-party
# content: they must never become HTML, code or a direct link.
set -u
cd "$(dirname "$0")/.."
DIRS="apps/desktop/src packages/viewer/src packages/export-shell/src"
fail=0

check() {
  # Ignores comments (// ... and * ... from doc blocks).
  hits=$(grep -rnE "$1" $DIRS --include='*.ts' --include='*.tsx' 2>/dev/null | grep -vE '^[^:]+:[0-9]+:[[:space:]]*(//|\*|/\*)')
  if [ -n "$hits" ]; then
    echo "FORBIDDEN ($2):"
    echo "$hits"
    fail=1
  fi
}

check 'dangerouslySetInnerHTML|\.innerHTML|outerHTML|insertAdjacentHTML|document\.write' "building HTML from text"
check '(^|[^.a-zA-Z])eval\(|new Function\(' "running dynamic code"
check 'href=\{' "dynamic href (message links go through the confirmation)"
check 'srcdoc|<iframe' "iframes"
check 'localStorage|sessionStorage|indexedDB' "persisting data in the browser"

# Freezing the main window's prototypes must be the first
# import (Tauri's freezePrototype stays off: it breaks the Google login).
first_import=$(grep -m1 '^import' apps/desktop/src/main.tsx)
if [ "$first_import" != 'import "./freeze";' ]; then
  echo "FORBIDDEN: apps/desktop/src/main.tsx must start with import \"./freeze\"; (found: $first_import)"
  fail=1
fi

if [ "$fail" -eq 0 ]; then echo "check-security: ok"; fi
exit "$fail"
