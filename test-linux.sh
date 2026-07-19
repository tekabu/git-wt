#!/usr/bin/env bash
# Run git-wt's tests on Linux via Docker.
#
#   ./linux-test.sh                  # build image, run unit + live tests
#   ./linux-test.sh --build-install  # verify build.sh + install-linux.sh on Linux
#   ./linux-test.sh --shell          # build image, drop into a shell
#   ./linux-test.sh --rebuild        # force image rebuild (no cache), then test
#   ./linux-test.sh --native         # already on Linux: test here, no Docker
#
# The container is a throwaway Debian box with Rust + git. Nothing is written
# back to the host; the Linux build target lives inside the image.
#
# --native skips Docker entirely and runs the suite in the current checkout.
# Only useful when the host IS Linux; the isolation modes (--shell,
# --build-install) still need the container, since their whole point is a box
# with no toolchain and a throwaway $HOME.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
image="git-wt-test"

build_args=()
mode="test"
native=0
for arg in "$@"; do
  case "$arg" in
    --shell)         mode="shell" ;;
    --build-install) mode="build-install" ;;
    --rebuild)       build_args+=(--no-cache) ;;
    --native)        native=1 ;;
    -h|--help)
      grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "error: unknown arg '$arg'" >&2; exit 1 ;;
  esac
done

if [ "$native" = 1 ]; then
  # --shell and --build-install write to $HOME and install binaries; running
  # them natively would touch the developer's real rc file and PATH.
  [ "$mode" = "test" ] || {
    echo "error: --native cannot be combined with --$mode (needs the container)" >&2; exit 1
  }
  [ "$(uname -s)" = "Linux" ] || echo "warning: --native on $(uname -s), not Linux" >&2
  echo "Running unit + live tests natively (no Docker)..."
  cd "$here"
  cargo test --release
  exec ./test.sh
fi

command -v docker >/dev/null 2>&1 || {
  echo "error: docker not found on PATH (on Linux, try --native)" >&2; exit 1
}

echo "Building image '$image' (Linux)..."
docker build ${build_args[@]+"${build_args[@]}"} -t "$image" "$here"

if [ "$mode" = "shell" ]; then
  echo "Dropping into container shell. Run: cargo test --release && ./test.sh"
  exec docker run --rm -it "$image" bash
fi

if [ "$mode" = "build-install" ]; then
  echo "Verifying build.sh, one-file installer, and install-linux.sh (source) on Linux..."
  exec docker run --rm -e SHELL=/bin/bash "$image" bash -euc '
    echo "=== build.sh: version + compile + one file ==="
    ./build.sh
    ls -1 dist/

    echo "=== one-file installer (no repo, no toolchain) ==="
    # Copy ONLY the self-installing script to an empty dir to prove isolation.
    inst="$(ls dist/git-wt-*.install.sh)"
    mkdir -p /tmp/only && cp "$inst" /tmp/only/
    cd /tmp/only && ./git-wt-*.install.sh --alias wt
    export PATH="$HOME/.local/bin:$PATH"
    echo "-- binary on PATH:"; command -v git-wt; git-wt version
    grep -q "# >>> git-wt alias >>>" "$HOME/.bashrc" || { echo "alias block missing" >&2; exit 1; }
    eval "$(sed -n "/# >>> git-wt alias >>>/,/# <<< git-wt alias <<</p" "$HOME/.bashrc")"
    mkdir -p /tmp/r/app && cd /tmp/r/app && git init -q && git commit -q --allow-empty -m i
    echo "-- wt list via alias:"; wt list

    echo "=== install-linux.sh: from source (cargo) ==="
    cd /work && ./install-linux.sh
    "${CARGO_HOME:-$HOME/.cargo}/bin/git-wt" version

    echo "OK: build.sh + one-file installer + source install verified"
  '
fi

echo "Running unit + live tests on Linux..."
exec docker run --rm "$image"
