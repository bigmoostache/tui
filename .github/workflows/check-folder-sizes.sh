#!/usr/bin/env bash
# Check that no directory has more than 8 entries.
# Usage: .github/workflows/check-folder-sizes.sh
set -euo pipefail

exit_code=0
while IFS= read -r dir; do
  count=$(find "$dir" -maxdepth 1 -mindepth 1 | wc -l)
  if [ "$count" -gt 8 ]; then
    echo "::error::$dir has $count entries (max 8)"
    echo "FAIL: $dir has $count entries (max 8)" >&2
    exit_code=1
  fi
done < <(find . -mindepth 1 -type d \
  -not -path './target/*' \
  -not -path '*/target/*' \
  -not -path './.git' \
  -not -path './.git/*' \
  -not -path './crates' \
  -not -path './.context-pilot' \
  -not -path './.context-pilot/*' \
  -not -path './website/*' \
  -not -path './docs' \
  -not -path './docs/*')
exit $exit_code
