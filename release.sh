#!/usr/bin/env bash
# Run the local release build, passing every argument straight through.
#
#   ./run.sh 1,2 meld        # == target/release/git-wt 1,2 meld
#   ./run.sh list
#   ./run.sh --help
#   ./run.sh -n add feat/x   # -n: skip the rebuild, run whatever is built
#
# Rebuilds first (a no-op cargo build when nothing changed), so what runs is
# always current source. Nothing is installed and PATH/rc files are untouched —
# the binary on your PATH stays whatever you installed.
#
# This is the raw binary, not the `wt` wrapper: switch/cd PRINT a path instead
# of changing your shell's directory. To cd with a dev build:
#     cd "$(./run.sh 2 path)"
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="$here/target/release/git-wt"

build=1
if [ "${1:-}" = "-n" ] || [ "${1:-}" = "--no-build" ]; then
  build=0
  shift
fi

if [ "$build" = 1 ]; then
  # Build chatter goes to stderr so it can't pollute the stdout path contract
  # (`cd "$(./run.sh 2 path)"` must capture the path and nothing else).
  cargo build --release --manifest-path "$here/Cargo.toml" >&2
fi

[ -x "$bin" ] || { echo "error: no build at $bin; run ./build.sh" >&2; exit 1; }

exec "$bin" "$@"
