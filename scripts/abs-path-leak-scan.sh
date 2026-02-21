#!/usr/bin/env bash
set -euo pipefail

if [ "${YORE_SKIP_ABSOLUTE_PATH_CHECK:-0}" = "1" ]; then
  exit 0
fi

repo_root="$(git rev-parse --show-toplevel)"
home_dir="${HOME:-}"

patch_file="$(mktemp)"
trap 'rm -f "$patch_file"' EXIT

git diff --cached --no-color > "$patch_file"
if [ ! -s "$patch_file" ]; then
  exit 0
fi

mask_path() {
  local line="$1"
  local masked="$line"

  if [ -n "$home_dir" ]; then
    masked="${masked//$home_dir/<HOME>}"
  fi
  masked="${masked//$repo_root/<REPO_ROOT>}"

  masked="$(printf '%s' "$masked" | perl -pe 's#/home/[^/[:space:]"]+#/home/<USER>/#g; s#/Users/[^/[:space:]"]+#/Users/<USER>/#g; s#[A-Za-z]:\\[^\\[:space:]]+#<WINDOWS_PATH>#g')"
  printf '%s' "$masked"
}

has_match=0
while IFS= read -r line; do
  is_home_path=0
  if printf '%s\n' "$line" | grep -Eq "/home/[A-Za-z0-9._-]+/[^[:space:]\\\"\\\']+"; then
    is_home_path=1
  elif printf '%s\n' "$line" | grep -Eq "/Users/[A-Za-z0-9._-]+/[^[:space:]\\\"\\\']+"; then
    is_home_path=1
  elif [ -n "$home_dir" ] && [[ "$line" == *"$home_dir"* ]]; then
    is_home_path=1
  elif [ -n "$repo_root" ] && [[ "$line" == *"$repo_root"* ]]; then
    is_home_path=1
  elif [[ "$line" =~ [A-Za-z]:\\ ]]; then
    is_home_path=1
  fi

  if [ "$is_home_path" -eq 1 ]; then
    has_match=1
    printf '%s\n' "abs-path-check: suspicious path in staged diff line: $(mask_path "$line")"
  fi
done < "$patch_file"

if [ "$has_match" -eq 1 ]; then
  echo "Absolute-path leak check failed:"
  echo "- Avoid hard-coding absolute host paths in staged changes."
  echo "- Use variables/placeholders instead of full home paths (for example \$HOME, \${REPO_ROOT}, relative paths)."
  echo "- If this is intentional and non-sensitive, set YORE_SKIP_ABSOLUTE_PATH_CHECK=1."
  exit 1
fi
