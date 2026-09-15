#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURE="${TMPDIR:-/tmp}/fastfind-ui-click"
EXPECTED="$FIXTURE/files/reports/QuarterlyReport2026.pdf"

pkill -x fastfind-gui 2>/dev/null || true
rm -rf "$FIXTURE"
mkdir -p "$FIXTURE/files/reports" "$FIXTURE/index"
printf 'fixture\n' > "$EXPECTED"
printf 'fixture\n' > "$FIXTURE/files/unrelated-notes.txt"

"$ROOT/target/debug/fastfind" \
  --database "$FIXTURE/index/index.db" \
  index "$FIXTURE/files" >/dev/null

uv run --no-project --with 'PyAutoGUI==0.9.54' python \
  "$ROOT/tests/ui_search_x11.py" \
  --binary "$ROOT/target/debug/fastfind-gui" \
  --database "$FIXTURE/index/index.db" \
  --query quarterly \
  --expected-path "$EXPECTED" \
  --screenshot "$FIXTURE/search-result.png"
