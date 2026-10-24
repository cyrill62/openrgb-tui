#!/usr/bin/env bash
# Sets the project version: writes VERSION, syncs Cargo.toml and Cargo.lock, turns the
# CHANGELOG.md "Unreleased" section into the new version, and commits the result.
#
# Usage: scripts/bump-version.sh [options] [major | minor | patch | X.Y.Z]
#
#   With no version, it is taken from the current git-flow branch (release/X.Y.Z or
#   hotfix/X.Y.Z), so the usual flow is:
#     git flow release start 0.2.0    (or: git flow hotfix start 0.1.3)
#     scripts/bump-version.sh
#     git flow release finish 0.2.0
#     scripts/release.sh
#   major / minor / patch bump the version currently in VERSION.
#
#   Options:
#     --no-commit   only edit the files
#     -h, --help    show this help
#
# Run automatically on "git flow release/hotfix start" with:
#   git config gitflow.path.hooks scripts/hooks
set -euo pipefail

cd "$(dirname "$0")/.."

die() { echo "error: $*" >&2; exit 1; }
info() { echo "==> $*"; }
usage() { sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'; exit 0; }
# Replaces $1 with stdin in place, keeping its permissions.
rewrite() { local tmp; tmp=$(mktemp); cat > "$tmp"; cat "$tmp" > "$1"; rm -f "$tmp"; }

COMMIT=1
ARG=""
while [ $# -gt 0 ]; do
  case "$1" in
    --no-commit) COMMIT=0 ;;
    -h | --help) usage ;;
    -*) die "unknown option '$1' (see --help)" ;;
    *) [ -z "$ARG" ] || die "more than one version given"; ARG=$1 ;;
  esac
  shift
done

SEMVER='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'
CURRENT=$(tr -d '[:space:]' < VERSION 2>/dev/null || true)
[ -n "$CURRENT" ] || CURRENT=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n1)

if [ -z "$ARG" ]; then
  branch=$(git rev-parse --abbrev-ref HEAD)
  release_prefix=$(git config gitflow.prefix.release || echo release/)
  hotfix_prefix=$(git config gitflow.prefix.hotfix || echo hotfix/)
  case "$branch" in
    "$release_prefix"* | "$hotfix_prefix"*) ARG=${branch#"$release_prefix"}; ARG=${ARG#"$hotfix_prefix"} ;;
    *) die "not on a git-flow release/hotfix branch; give a version (see --help)" ;;
  esac
  ARG=${ARG#v}
fi

case "$ARG" in
  major | minor | patch)
    [[ "$CURRENT" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+) ]] || die "current version '$CURRENT' is not X.Y.Z"
    maj=${BASH_REMATCH[1]} min=${BASH_REMATCH[2]} pat=${BASH_REMATCH[3]}
    case "$ARG" in
      major) NEW="$((maj + 1)).0.0" ;;
      minor) NEW="$maj.$((min + 1)).0" ;;
      patch) NEW="$maj.$min.$((pat + 1))" ;;
    esac ;;
  *) NEW=${ARG#v} ;;
esac
[[ "$NEW" =~ $SEMVER ]] || die "'$NEW' is not a semantic version (X.Y.Z)"
[ "$NEW" != "$CURRENT" ] || die "version is already $NEW"

TAG_PREFIX=$(git config gitflow.prefix.versiontag || echo v)
info "bumping $CURRENT -> $NEW"

echo "$NEW" > VERSION

# Only the [package] version, not dependency versions.
awk -v v="$NEW" '
  /^\[/ { pkg = ($0 == "[package]") }
  pkg && !done && /^version *=/ { print "version = \"" v "\""; done = 1; next }
  { print }
' Cargo.toml | rewrite Cargo.toml

# Our own entry in Cargo.lock, edited directly so this works offline and without cargo.
if [ -f Cargo.lock ]; then
  name=$(sed -n 's/^name *= *"\(.*\)"/\1/p' Cargo.toml | head -n1)
  awk -v n="$name" -v v="$NEW" '
    $0 == "name = \"" n "\"" { print; getline; if ($0 ~ /^version = /) $0 = "version = \"" v "\"" }
    { print }
  ' Cargo.lock | rewrite Cargo.lock
fi

# "## [Unreleased]" keeps its place; its entries move under the new version heading,
# and the compare links at the bottom follow.
if grep -qF "## [$NEW]" CHANGELOG.md; then
  info "CHANGELOG.md already has a $NEW section, left unchanged"
elif grep -q '^## \[Unreleased\]' CHANGELOG.md; then
  awk -v v="$NEW" -v d="$(date +%Y-%m-%d)" -v p="$TAG_PREFIX" '
    /^## \[Unreleased\]/ { print; print ""; print "## [" v "] - " d; next }
    /^\[Unreleased\]: / {
      base = $2; sub(/compare\/.*/, "", base)
      prev = $2; sub(/.*compare\//, "", prev); sub(/\.\.\.HEAD$/, "", prev)
      print "[Unreleased]: " base "compare/" p v "...HEAD"
      print "[" v "]: " base "compare/" prev "..." p v
      next
    }
    { print }
  ' CHANGELOG.md | rewrite CHANGELOG.md
else
  info "warning: no '## [Unreleased]' section in CHANGELOG.md, left unchanged"
fi

if [ $COMMIT -eq 1 ]; then
  git add VERSION Cargo.toml CHANGELOG.md
  [ -f Cargo.lock ] && git add Cargo.lock
  git commit -m "Bump version to $NEW"
  info "committed; review CHANGELOG.md, then finish the release/hotfix with git flow"
else
  info "files updated (not committed)"
fi
