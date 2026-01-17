#!/usr/bin/env bash
set -e

mapfile -d '' FILES < <(
  git ls-files -z --cached --others --exclude-standard
)

PROBLEM_FILES=()

for f in "${FILES[@]}"; do
  [[ -f "$f" ]] || continue
  if LC_ALL=C grep -Iq . "$f" && grep -q $'\r' "$f"; then
    PROBLEM_FILES+=("$f")
  fi
done

if [[ ${#PROBLEM_FILES[@]} -eq 0 ]]; then
  echo "No files with CRLF line endings found."
  exit 0
fi

echo "Files with CRLF line endings:"
printf '  %s\n' "${PROBLEM_FILES[@]}"
echo

read -rp "Convert CRLF to LF using sed? [y/N] " ans
case "$ans" in
  y|Y|yes|YES) ;;
  *) echo "Aborted."; exit 1 ;;
esac

for f in "${PROBLEM_FILES[@]}"; do
  # CRLF → LF (portable)
  sed -i.bak 's/\r$//' "$f"
  rm -f "$f.bak"
  echo "normalized: $f"
done

echo "Done."
