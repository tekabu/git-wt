#!/usr/bin/env bash
# Build a release binary, optionally bumping the crate version first.
#
#   ./build.sh                 # build release at current version
#   ./build.sh 1.2.3           # set version to 1.2.3, then build
#   ./build.sh patch           # bump x.y.Z, then build
#   ./build.sh minor           # bump x.Y.0, then build
#   ./build.sh major           # bump X.0.0, then build
#
# The chosen version is written to Cargo.toml so it flows into --version.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
manifest="$here/Cargo.toml"
arg="${1:-}"

cur="$(grep -m1 '^version = ' "$manifest" | sed -E 's/version = "([^"]+)"/\1/')"
[ -n "$cur" ] || { echo "error: cannot read version from $manifest" >&2; exit 1; }

bump() {  # $1 = current x.y.z, $2 = major|minor|patch -> new x.y.z
  IFS=. read -r x y z <<<"$1"
  case "$2" in
    major) echo "$((x+1)).0.0" ;;
    minor) echo "$x.$((y+1)).0" ;;
    patch) echo "$x.$y.$((z+1))" ;;
  esac
}

case "$arg" in
  "")                    new="$cur" ;;
  major|minor|patch)     new="$(bump "$cur" "$arg")" ;;
  [0-9]*.[0-9]*.[0-9]*)
    # The glob allows trailing junk (e.g. 1.2.3xyz); reject anything that is
    # not strictly digits and dots.
    case "$arg" in
      *[!0-9.]*) echo "error: '$arg' is not a valid x.y.z version" >&2; exit 1 ;;
    esac
    new="$arg" ;;
  *) echo "error: '$arg' is not a version or major|minor|patch" >&2; exit 1 ;;
esac

if [ "$new" != "$cur" ]; then
  # Confirm before rewriting the manifest version.
  printf 'Change version %s -> %s? [y/N] ' "$cur" "$new" >&2
  read -r reply
  case "$reply" in
    y|Y|yes|YES) ;;
    *) echo "Aborted; version unchanged." >&2; exit 1 ;;
  esac
  # Rewrite only the first `version = "..."` line (the [package] one).
  tmp="$(mktemp)"
  awk -v v="$new" 'BEGIN{done=0}
    !done && /^version = "/ {print "version = \"" v "\""; done=1; next}
    {print}' "$manifest" > "$tmp"
  mv "$tmp" "$manifest"
  echo "Version $cur -> $new"
fi

echo "Building release..."
cargo build --release --manifest-path "$manifest"
echo "Built git-wt $new at $here/target/release/git-wt"
