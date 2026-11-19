#!/bin/sh
# Barra padrões perigosos no código do frontend. Mensagens são conteúdo de
# terceiros: nunca podem virar HTML, código ou link direto.
set -u
cd "$(dirname "$0")/.."
DIRS="apps/desktop/src packages/viewer/src packages/export-shell/src"
fail=0

check() {
  # Ignora comentários (// ... e * ... de blocos de documentação).
  hits=$(grep -rnE "$1" $DIRS --include='*.ts' --include='*.tsx' 2>/dev/null | grep -vE '^[^:]+:[0-9]+:[[:space:]]*(//|\*|/\*)')
  if [ -n "$hits" ]; then
    echo "PROIBIDO ($2):"
    echo "$hits"
    fail=1
  fi
}

check 'dangerouslySetInnerHTML|\.innerHTML|outerHTML|insertAdjacentHTML|document\.write' "montar HTML a partir de texto"
check '(^|[^.a-zA-Z])eval\(|new Function\(' "executar código dinâmico"
check 'href=\{' "href dinâmico (links de mensagem passam pela confirmação)"
check 'srcdoc|<iframe' "iframes"
check 'localStorage|sessionStorage|indexedDB' "persistir dados no navegador"

if [ "$fail" -eq 0 ]; then echo "check-security: ok"; fi
exit "$fail"
