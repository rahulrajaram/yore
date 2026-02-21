#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
if [ -z "$repo_root" ]; then
  echo "Not inside a git repository."
  exit 1
fi

git config core.hooksPath "$repo_root/.githooks"
chmod +x "$repo_root/.githooks/pre-commit" "$repo_root/.githooks/commit-msg"
echo "Git hooks installed. Commit-time checks now use:"
echo " - $repo_root/.githooks/pre-commit"
echo " - $repo_root/.githooks/commit-msg"
