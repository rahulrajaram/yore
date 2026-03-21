#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
if [ -z "$repo_root" ]; then
  echo "Not inside a git repository."
  exit 1
fi

git_dir="$(git rev-parse --git-dir)"
if [ -d "$git_dir/rebase-merge" ] || [ -d "$git_dir/rebase-apply" ] || [ -f "$git_dir/MERGE_HEAD" ]; then
  echo "Cannot install hooks during an active rebase or merge." >&2
  exit 1
fi

source_dir="${COMMITHOOKS_DIR:-${HOME:-/home/$USER}/Documents/commithooks}"
if [ ! -d "$source_dir/lib" ] || [ ! -f "$source_dir/lib/common.sh" ]; then
  echo "Commithooks source is invalid: $source_dir" >&2
  echo "Expected at least: $source_dir/lib/common.sh" >&2
  exit 1
fi

printf 'Commithooks installer state\n'
printf '  Repo root:        %s\n' "$repo_root"
printf '  Git dir:          %s\n' "$git_dir"
printf '  Source:           %s\n' "$source_dir"
printf '  core.hooksPath:   %s\n' "$(git config --get core.hooksPath || echo '<unset>')"

copy_dispatcher() {
  local hook="$1"
  local src="$source_dir/$hook"
  local dst="$git_dir/hooks/$hook"
  local sample="$dst.sample"

  if [ ! -f "$src" ]; then
    printf '  [skip] %s (missing in source)\n' "$hook"
    return
  fi

  if [ -f "$dst" ]; then
    local sample_content=""
    if [ -f "$sample" ]; then
      sample_content="$(cat "$sample")"
    fi

    if [ "$(cat "$dst")" != "$sample_content" ]; then
      printf '  [skip] %s (existing custom hook)\n' "$hook"
      return
    fi
  fi

  cp "$src" "$dst"
  chmod +x "$dst"
  printf '  [ok]   %s\n' "$hook"
}

printf 'Dispatchers (.git/hooks/):\n'
mkdir -p "$git_dir/hooks"
for hook in pre-commit commit-msg pre-push post-checkout post-merge; do
  copy_dispatcher "$hook"
done

rm -rf "${git_dir:?}/lib"
cp -r "$source_dir/lib" "$git_dir/lib"
printf 'Library (.git/lib/):\n'
printf '  [ok]   %s modules copied\n' "$(find "$git_dir/lib" -maxdepth 1 -type f | wc -l | tr -d ' ')"

git config --unset core.hooksPath 2>/dev/null || true
printf '  core.hooksPath unset; using .git/hooks dispatchers\n'

for hook in pre-commit commit-msg pre-push post-checkout post-merge; do
  if [ -f "$repo_root/.githooks/$hook" ]; then
    chmod +x "$repo_root/.githooks/$hook"
  fi
done

printf 'Repo-local hooks:\n'
for hook in pre-commit commit-msg pre-push post-checkout post-merge; do
  if [ -x "$repo_root/.githooks/$hook" ]; then
    printf '  [ok]   .githooks/%s\n' "$hook"
  elif [ -x "$repo_root/scripts/git-hooks/$hook" ]; then
    printf '  [ok]   scripts/git-hooks/%s\n' "$hook"
  else
    printf '  [warn] no repo-local implementation for %s\n' "$hook"
  fi
done
