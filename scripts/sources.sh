#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-23

# Every Relay build uses one commit of each sibling repository it includes, written in a lock file at the
# repository root: stream.lock (OpenXPlay-Stream) and diplay.lock (OpenXPlay-DiPlay). The workflows read and
# check the locks with this script, and a lock moves only with `bump`, so both repositories follow one rule set.
#
#   sources.sh read <repo>                  print the locked commit
#   sources.sh verify <repo> <dir>          fail unless the checkout at <dir> is the locked commit
#   sources.sh on-main [--warn] <repo>...   fail (or only warn) unless each locked commit is on its main on GitHub
#   sources.sh summary <relay-commit>       list, in Markdown, the commits each lock gained since <relay-commit>
#   sources.sh bump <repo> [dir]            lock the HEAD of a clean checkout that is on origin/main
#
# <repo> is stream or diplay. on-main and summary need the GitHub CLI (GH_TOKEN in workflows).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
level=error

fail() {
  if [ "${GITHUB_ACTIONS:-}" = true ]; then echo "::$level::$*" >&2; else echo "sources.sh: $*" >&2; fi
  exit 1
}

repository() {
  case $1 in
    stream) echo OpenXPlay-Stream ;;
    diplay) echo OpenXPlay-DiPlay ;;
    *) fail "Unknown repository '$1'; use stream or diplay." ;;
  esac
}

locked() {
  local name file sha
  name=$(repository "$1")
  file="$root/$1.lock"
  [ -f "$file" ] || fail "$1.lock is missing."
  sha=$(tr -d '[:space:]' < "$file")
  [[ $sha =~ ^[0-9a-f]{40}$ ]] || fail "$1.lock must hold one full $name commit hash."
  echo "$sha"
}

verify() {
  local name sha head
  name=$(repository "$1")
  sha=$(locked "$1")
  head=$(git -C "$2" rev-parse HEAD) || fail "$2 is not a $name checkout."
  [ "$head" = "$sha" ] || fail "$name at $2 is ${head:0:12}, not ${sha:0:12} from $1.lock."
}

# Status of <sha>...main is "ahead" or "identical" exactly when <sha> is an ancestor of main. Runs in a subshell
# under ||, where set -e does not apply, hence the explicit exits.
on_main() {
  local name sha status
  name=$(repository "$1") || exit 1
  sha=$(locked "$1") || exit 1
  status=$(gh api "repos/OpenXPlay/$name/compare/$sha...main?per_page=1" --jq .status) ||
    fail "Cannot compare $name ${sha:0:12} from $1.lock with main on GitHub; is it pushed?"
  case $status in
    ahead | identical) ;;
    *) fail "$name ${sha:0:12} from $1.lock is not on main ($status); push it to main or lock a commit that is." ;;
  esac
}

summary() {
  local base=$1 repo name old new
  if [[ ! $base =~ ^[0-9a-f]{40}$ || $base =~ ^0+$ ]] || ! git -C "$root" cat-file -e "$base^{commit}" 2>/dev/null; then
    return 0
  fi
  for repo in stream diplay; do
    name=$(repository "$repo")
    new=$(locked "$repo")
    old=$(git -C "$root" show "$base:$repo.lock" 2>/dev/null | tr -d '[:space:]') || old=""
    [ "$old" != "$new" ] || continue
    if [ -z "$old" ]; then
      echo "### $name locked at \`${new:0:12}\`"
      continue
    fi
    echo "### $name \`${old:0:12}\` → \`${new:0:12}\`"
    gh api "repos/OpenXPlay/$name/compare/$old...$new" --jq \
      '"\(.status), \(.total_commits) commits:", (.commits[] | "- `\(.sha[0:12])` \(.commit.message | split("\n")[0])")' ||
      echo "The commits in between cannot be listed."
  done
}

bump() {
  local name dir sha old
  name=$(repository "$1")
  dir=${2:-$root/../$name}
  [ -d "$dir" ] || fail "No $name checkout at $dir; pass its path."
  [ -z "$(git -C "$dir" status --porcelain)" ] || fail "$name at $dir has uncommitted changes."
  sha=$(git -C "$dir" rev-parse HEAD)
  git -C "$dir" fetch --quiet origin main
  git -C "$dir" merge-base --is-ancestor "$sha" FETCH_HEAD || fail "$name ${sha:0:12} is not on origin/main; push it first."
  old=""
  [ ! -f "$root/$1.lock" ] || old=$(tr -d '[:space:]' < "$root/$1.lock")
  if [ "$old" = "$sha" ]; then
    echo "$1.lock already holds ${sha:0:12}."
    return 0
  fi
  echo "$sha" > "$root/$1.lock"
  echo "$1.lock: ${old:0:12} -> ${sha:0:12}"
  [ -z "$old" ] || git -C "$dir" log --oneline --no-decorate "$old..$sha" || true
  echo "Commit it as: Pin $name ${sha:0:12}: <why>"
}

command=${1:-}
[ $# -eq 0 ] || shift
case $command in
  read) [ $# -eq 1 ] || fail "Usage: sources.sh read <repo>"; locked "$1" ;;
  verify) [ $# -eq 2 ] || fail "Usage: sources.sh verify <repo> <dir>"; verify "$1" "$2" ;;
  on-main)
    if [ "${1:-}" = --warn ]; then
      level=warning
      shift
    fi
    [ $# -ge 1 ] || fail "Usage: sources.sh on-main [--warn] <repo>..."
    failed=0
    for repo in "$@"; do (on_main "$repo") || failed=1; done
    [ $level = warning ] || exit "$failed"
    ;;
  summary) [ $# -eq 1 ] || fail "Usage: sources.sh summary <relay-commit>"; summary "$1" ;;
  bump) [ $# -ge 1 ] && [ $# -le 2 ] || fail "Usage: sources.sh bump <repo> [dir]"; bump "$@" ;;
  *) fail "Usage: sources.sh read|verify|on-main|summary|bump ...; see the top of scripts/sources.sh." ;;
esac
