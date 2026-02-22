#!/usr/bin/env bash
set -euo pipefail

if [ "${YORE_SKIP_ABSOLUTE_PATH_CHECK:-0}" = "1" ]; then
  exit 0
fi

repo_root="$(git rev-parse --show-toplevel)"
home_dir="${HOME:-}"
mask_output="${YORE_ABS_PATH_MASK:-1}"

patch_file="$(mktemp)"
trap 'rm -f "$patch_file"' EXIT

git diff --cached --no-color > "$patch_file"
if [ ! -s "$patch_file" ]; then
  exit 0
fi

mask_path() {
  local line="$1"
  local masked="$line"

  if [ "${YORE_ABS_PATH_MASK:-1}" = "1" ]; then
    if [ -n "$home_dir" ]; then
      masked="${masked//$home_dir/<HOME>}"
    fi
    if [ -n "$repo_root" ]; then
      masked="${masked//$repo_root/<REPO_ROOT>}"
    fi

    masked="$(printf '%s' "$masked" | perl -pe "s#/home/[^/[:space:]\"']+#/home/<USER>/#g; s#/Users/[^/[:space:]\"']+#/Users/<USER>/#g; s#[A-Za-z]:\\\\[^[:space:]\"']+#<WINDOWS_PATH>#g; s#/tmp/[^/[:space:]\"']*#/tmp/<PATH>#g; s#/opt/[^/[:space:]\"']*#/opt/<PATH>#g; s#/var/[^/[:space:]\"']*#/var/<PATH>#g; s#/etc/[^/[:space:]\"']*#/etc/<PATH>#g; s#/mnt/[^/[:space:]\"']*#/mnt/<PATH>#g")"
  fi

  printf '%s' "$masked"
}

report_line() {
  local line="$1"
  if [ "$mask_output" = "0" ]; then
    printf '%s\n' "$line"
  else
    printf '%s\n' "$(mask_path "$line")"
  fi
}

is_absolute_path() {
  local line="$1"

  if [ -n "$home_dir" ] && [[ "$line" == *"$home_dir"* ]]; then
    return 0
  fi

  if [ -n "$repo_root" ] && [[ "$line" == *"$repo_root"* ]]; then
    return 0
  fi

  if printf '%s\n' "$line" | grep -Eq "(^|[[:space:]\"'\\(\\[])file:///[^[:space:]\"'\\)\\]\\}>]+"; then
    return 0
  fi

  if printf '%s\n' "$line" | grep -Eq "(^|[[:space:]\"'\\(\\[])(/home/[A-Za-z0-9._-]+/[^[:space:]\"'\\)\\]\\}>]+)"; then
    return 0
  fi

  if printf '%s\n' "$line" | grep -Eq "(^|[[:space:]\"'\\(\\[])(/Users/[A-Za-z0-9._-]+/[^[:space:]\"'\\)\\]\\}>]+)"; then
    return 0
  fi

  if printf '%s\n' "$line" | grep -Eq "(^|[[:space:]\"'\\(\\[])(/tmp/[A-Za-z0-9._/-]+|/opt/[A-Za-z0-9._/-]+|/var/[A-Za-z0-9._/-]+|/etc/[A-Za-z0-9._/-]+|/mnt/[A-Za-z0-9._/-]+|/var/[^[:space:]\"'\\)\\]\\}>]+)"; then
    return 0
  fi

  if printf '%s\n' "$line" | grep -Eq "(^|[[:space:]\"'\\(\\[])([A-Za-z]:\\\\[^[:space:]\"'\\)\\]\\}>]+)"; then
    return 0
  fi

  if printf '%s\n' "$line" | grep -Eq "(^|[[:space:]\"'\\(\\[])(\\\\[^/[:space:]\"'\\)\\]\\}>]+)"; then
    return 0
  fi

  return 1
}

has_match=0
while IFS= read -r line; do
  if is_absolute_path "$line"; then
    has_match=1
    report_line "abs-path-check: suspicious path in staged diff line: $line"
  fi
done < "$patch_file"

if [ "$has_match" -eq 1 ]; then
  echo "Absolute-path leak check failed:"
  echo "- Avoid hard-coding absolute host paths in staged changes."
  echo "- Use environment variables or placeholders (for example \$HOME, \$REPO_ROOT, or relative paths)."
  echo "- Keep raw paths visible while debugging with YORE_ABS_PATH_MASK=0."
  echo "- If this is intentional and non-sensitive, set YORE_SKIP_ABSOLUTE_PATH_CHECK=1."
  exit 1
fi
