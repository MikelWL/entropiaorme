#!/usr/bin/env bash
# Resolve the base..head range a diff-scoped guard inspects, from the workflow
# event, and say when there is nothing to inspect.
#
# A pull request supplies its base..head through PR_BASE_SHA / PR_HEAD_SHA. A
# push supplies the pushed commits' before..after through PUSH_BEFORE_SHA /
# PUSH_HEAD_SHA. A push that creates a branch has no before (git reports the
# all-zero SHA, and the event carries created=true): it stands up content that
# already exists on another ref, so there is no change to inspect and the guard
# skips cleanly rather than failing. Any other event, or a handled event missing
# a SHA, is an error: the guards fail closed on a range they cannot resolve.
#
# Emits base, head, and skip (true/false) as step outputs. Run from a workflow
# step after the checkout, from the repository root:
#   EVENT_NAME=push PUSH_BEFORE_SHA=<before> PUSH_HEAD_SHA=<after> \
#     bash .github/scripts/resolve-change-range.sh

set -euo pipefail

zero_sha="0000000000000000000000000000000000000000"

case "${EVENT_NAME:-}" in
  pull_request) base="${PR_BASE_SHA:-}"; head="${PR_HEAD_SHA:-}" ;;
  push) base="${PUSH_BEFORE_SHA:-}"; head="${PUSH_HEAD_SHA:-}" ;;
  *) base=""; head="" ;;
esac

emit() {
  # base head skip
  if [ -n "${GITHUB_OUTPUT:-}" ]; then
    {
      echo "base=$1"
      echo "head=$2"
      echo "skip=$3"
    } >> "$GITHUB_OUTPUT"
  fi
}

if [ "${PUSH_CREATED:-false}" = "true" ] || [ "$base" = "$zero_sha" ]; then
  echo "resolve-change-range: the push created its branch, so there is no prior state to compare; the diff-scoped guard is skipped."
  emit "" "$head" true
  exit 0
fi

if [ -z "$base" ] || [ -z "$head" ]; then
  echo "resolve-change-range: no base..head range for event '${EVENT_NAME:-}'; failing closed." >&2
  exit 1
fi

if ! git cat-file -e "$base^{commit}" 2>/dev/null; then
  echo "resolve-change-range: base commit $base is not in the checkout (was the ref rewritten?); failing closed." >&2
  exit 1
fi

echo "resolve-change-range: $base..$head"
emit "$base" "$head" false
