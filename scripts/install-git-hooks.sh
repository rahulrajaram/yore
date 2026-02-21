#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
if [ -z "$repo_root" ]; then
  echo "Not inside a git repository."
  exit 1
fi

shared_hooks="${COMMITHOOKS_DIR:-${HOME:-/home/$USER}/Documents/commithooks}"
repo_local_installer="$repo_root/scripts/git-hooks/install-git-hooks.sh"

if [ -x "$repo_local_installer" ]; then
  if [ -d "$shared_hooks" ]; then
    COMMITHOOKS_DIR="$shared_hooks" "$repo_local_installer"
  else
    "$repo_local_installer"
  fi
  exit 0
fi

if [ -d "$shared_hooks" ]; then
  git config core.hooksPath "$shared_hooks"
  chmod +x "$shared_hooks/pre-commit" "$shared_hooks/commit-msg"
  echo "Git hooks installed from shared path."
  echo " - pre-commit: $shared_hooks/pre-commit"
  echo " - commit-msg: $shared_hooks/commit-msg"
  exit 0
fi

git config core.hooksPath "$repo_root/.githooks"
chmod +x "$repo_root/.githooks/pre-commit" "$repo_root/.githooks/commit-msg"
echo "Git hooks installed. Commit-time checks now use:"
echo " - $repo_root/.githooks/pre-commit"
echo " - $repo_root/.githooks/commit-msg"
