#!/usr/bin/env bash
set -euo pipefail

base_ref="${YORE_BASE_REF:-}"
head_ref="${YORE_HEAD_REF:-HEAD}"
needs_rewrite=0
ai_enabled="${YORE_SQUASH_AI_ENABLED:-0}"
output_file="$(mktemp)"

cleanup() {
  rm -f "$output_file"
}
trap cleanup EXIT

append_report() {
  printf '%s\n' "$1" | tee -a "$output_file" >/dev/null
}

append_pair() {
  local a_short=$1
  local b_short=$2
  local overlap=$3
  local score=$4
  local reason=$5
  needs_rewrite=1
  append_report "- \`$a_short\` \`$b_short\`: $reason (${score}% file-overlap, $overlap shared files)"
}

detect_base() {
  if [ -n "$base_ref" ]; then
    return 0
  fi

  if [ "${GITHUB_EVENT_NAME:-}" = "pull_request" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
    base_ref="origin/${GITHUB_BASE_REF}"
    if ! git cat-file -e "${base_ref}^{commit}" >/dev/null 2>&1; then
      git fetch --quiet origin "${GITHUB_BASE_REF}" || true
    fi
    if git cat-file -e "${base_ref}^{commit}" >/dev/null 2>&1; then
      return 0
    fi
  fi

  if [ "${GITHUB_EVENT_NAME:-}" = "push" ] && [ -n "${GITHUB_EVENT_BEFORE:-}" ]; then
    base_ref="$GITHUB_EVENT_BEFORE"
    return 0
  fi

  if [ -n "${GITHUB_BASE_SHA:-}" ] && [ -n "${GITHUB_HEAD_SHA:-}" ]; then
    base_ref="$GITHUB_BASE_SHA"
    head_ref="$GITHUB_HEAD_SHA"
    return 0
  fi

  upstream=$(git rev-parse --abbrev-ref --symbolic-full-name @{u} 2>/dev/null || true)
  if [ -n "$upstream" ]; then
    base_ref="$upstream"
    return 0
  fi

  base_ref=""
}

format_commit_subject_prefix() {
  echo "$1" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9 ]+/ /g' | awk '{print $1" "$2}' | sed 's/^ *//; s/  */ /; s/ $//'
}

collect_changed_files() {
  git show --pretty=format: --name-only --no-color "$1" | sed '/^$/d' | sort -u
}

emit_ai_review() {
  if [ "$ai_enabled" != "1" ]; then
    return 0
  fi

  local ai_helper="${YORE_AI_HELPER:-}"
  if [ -z "$ai_helper" ]; then
    append_report ""
    append_report "AI review skipped: set YORE_AI_HELPER and related vars to enable LLM-assisted review."
    return 0
  fi

  if ! command -v "$ai_helper" >/dev/null 2>&1; then
    append_report ""
    append_report "AI review skipped: command '$ai_helper' was not found."
    return 0
  fi

  local -a ai_args=()
  if [ -n "${YORE_AI_ARGS:-}" ]; then
    read -r -a ai_args <<< "${YORE_AI_ARGS}"
  fi

  local ai_prompt="Using the following commit summaries, identify whether these commits should be squashed and how to group them:\n\n${analysis_payload}"
  local ai_output=""
  local ai_mode="${YORE_AI_INPUT_MODE:-stdin}"
  local status=0

  if [ "$ai_mode" = "arg" ]; then
    if ! ai_output=$("$ai_helper" "${ai_args[@]}" "$ai_prompt" 2>&1); then
      status=$?
    fi
  else
    if ! ai_output=$(printf '%s\n' "$ai_prompt" | "$ai_helper" "${ai_args[@]}" 2>&1); then
      status=$?
    fi
  fi

  append_report ""
  append_report "## AI suggestion"
  if [ "$status" -eq 0 ] && [ -n "$ai_output" ]; then
    append_report "$ai_output"
  else
    append_report "AI review failed with exit code $status. Continue with heuristic report above."
  fi
}

detect_base

if [ -z "$base_ref" ]; then
  append_report "Unable to determine a valid base commit range for squash review."
  append_report "SQUASH_SCOPE_NEEDS_REWRITE=0"
  cat "$output_file"
  exit 0
fi

if ! git cat-file -e "${base_ref}^{commit}" >/dev/null 2>&1; then
  append_report "Base ref '$base_ref' is unavailable locally."
  append_report "SQUASH_SCOPE_NEEDS_REWRITE=0"
  cat "$output_file"
  exit 0
fi
if ! git cat-file -e "${head_ref}^{commit}" >/dev/null 2>&1; then
  head_ref=HEAD
fi

range="${base_ref}..${head_ref}"
commit_count=$(git rev-list --count "$range" 2>/dev/null || echo 0)
if [ "$commit_count" -le 1 ]; then
  append_report "No squash opportunity: only 1 commit in range ($range)."
  append_report "SQUASH_SCOPE_NEEDS_REWRITE=0"
  cat "$output_file"
  exit 0
fi

mapfile -t commits < <(git rev-list --reverse "$range")
analysis_payload="Analyzed ${#commits[@]} commits between $base_ref and $head_ref for squash opportunities.\n\n"
append_report "# Commit squash-scope review"
append_report ""
append_report "Range: \`$range\`"
append_report "Commits reviewed: ${#commits[@]}"
append_report ""
append_report "## Heuristic candidates"

prev_commit=""
prev_files_tmp=""
prev_prefix=""
prev_short=""

for commit in "${commits[@]}"; do
  short=$(git rev-parse --short "$commit")
  subject=$(git log -1 --pretty=%s "$commit")
  files_tmp=$(mktemp)
  collect_changed_files "$commit" > "$files_tmp"
  changed_count=$(wc -l < "$files_tmp")
  prefix=$(format_commit_subject_prefix "$subject")

  if [ -n "$prev_commit" ]; then
    overlap=0
    while IFS= read -r file; do
      if [ -n "$file" ] && grep -Fxq "$file" "$prev_files_tmp"; then
        overlap=$((overlap + 1))
      fi
    done < "$files_tmp"

    if [ "$changed_count" -gt 0 ] || [ "$prev_count" -gt 0 ]; then
      if [ "$changed_count" -lt "$prev_count" ]; then
        min_changed=$changed_count
      else
        min_changed=$prev_count
      fi
      if [ "$min_changed" -eq 0 ]; then
        score=0
      else
        score=$((overlap * 100 / min_changed))
      fi
    else
      score=0
    fi

    if [ "$score" -ge 55 ] || { [ -n "$prefix" ] && [ "$prefix" = "$prev_prefix" ]; } ; then
      reason="similar scope"
      [ "$score" -ge 55 ] && reason="shared file overlap"
      [ "$score" -lt 55 ] && reason+=" and matching subject prefix"
      append_pair "$prev_short" "$short" "$overlap" "$score" "$reason"
    else
      analysis_payload+="No candidate between \`$prev_short\` and \`$short\`"
      if [ "$overlap" -gt 0 ]; then
        analysis_payload+=" (shared files: $overlap)."
      else
        analysis_payload+="."
      fi
      analysis_payload+="\n"
    fi
  else
    analysis_payload+="First commit in range: \`$short\`"
    analysis_payload+=" - $subject\n"
  fi

  analysis_payload+="\`$short\` $subject\n"

  if [ -n "$prev_files_tmp" ]; then
    rm -f "$prev_files_tmp"
  fi
  prev_commit="$commit"
  prev_files_tmp="$files_tmp"
  prev_count=$changed_count
  prev_prefix="$prefix"
  prev_short="$short"
done

if [ -n "$prev_files_tmp" ]; then
  rm -f "$prev_files_tmp"
fi

if [ "$needs_rewrite" -eq 0 ]; then
  append_report "No clear squash candidates found from overlap or message-prefix heuristics."
else
  append_report ""
  append_report "_These are heuristics only. Use your judgment before collapsing history._"
fi

emit_ai_review

append_report ""
append_report "SQUASH_SCOPE_NEEDS_REWRITE=$needs_rewrite"
cat "$output_file"

if [ -n "${GITHUB_OUTPUT:-}" ]; then
  marker="SQUASH_SCOPE_REPORT_$(date +%s)"
  {
    echo "squash_report<<$marker"
    cat "$output_file"
    echo "$marker"
    echo "needs_rewrite=$needs_rewrite"
  } >> "$GITHUB_OUTPUT"
fi
