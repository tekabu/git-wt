# git-wt

Create and manage git worktrees under the main worktree's `.worktrees/`
directory, named `<repo-folder>-<branch>`.

Installed on PATH as `git-wt`, so it also works as `git wt`.

```
~/code/myapp  +  feature/login  ->  ~/code/myapp/.worktrees/myapp-feature-login
```

Full command reference: [docs/COMMANDS.md](docs/COMMANDS.md).

## Install

### From the one-file installer (no Rust needed)

Download the single self-installing script for your platform —
`git-wt-<version>-<os>-<arch>.install.sh` — and run it. Nothing else: no repo,
no tarball, no toolchain. The binary is embedded inside the script.

```sh
chmod +x git-wt-1.0.9-linux-x86_64.install.sh
./git-wt-1.0.9-linux-x86_64.install.sh            # binary only
./git-wt-1.0.9-linux-x86_64.install.sh --alias wt # + a `wt` shell function
```

Installs to `~/.local/bin` (override with `GITWT_PREFIX=/usr/local`). Make sure
that `bin` dir is on your `PATH`. If another `git-wt` earlier on `PATH` shadows
the installed one, the script warns and suggests removing or symlinking it.

### From source

Needs Rust (`cargo`). Builds and installs in one step with `cargo install`,
dropping the binary in `~/.cargo/bin`:

```sh
./install.sh                 # build + install from source
./install.sh --alias wt      # + a `wt` shell function that cd's for you
```

If `cargo --version` fails, install Rust via [rustup](https://rustup.rs):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
cargo --version              # confirm
```

macOS/Linux only. On Windows, download and run
[`rustup-init.exe`](https://rustup.rs) instead. Accept the default (`1`) — it
edits your shell rc so `~/.cargo/bin` is on `PATH`.

**rustup does not install a linker.** A fresh machine fails the first build
with `error: linker 'cc' not found`. Install the platform's build tools once:

| Platform | Command |
|---|---|
| Debian/Ubuntu | `sudo apt install -y build-essential` |
| Fedora/RHEL | `sudo dnf groupinstall "Development Tools"` |
| Arch | `sudo pacman -S base-devel` |
| Alpine | `sudo apk add build-base` |
| macOS | `xcode-select --install` |

Only the source install needs this — the one-file installer ships a prebuilt
binary and needs no toolchain at all.

### `git-wt` (binary) vs `wt` (wrapper)

Two names, one tool, one behavioral difference: `cd`. A binary cannot change
its parent shell's directory, so `git-wt` always *prints* a path; the `wt`
shell function *cd's* using that path.

- **Interactive:** install with `--alias wt`, then use `wt`. `wt switch 1`
  drops you in the worktree; `wt add feat/x` creates it and drops you in;
  `wt remove 1` returns you to main.
- **Scripts:** call `git-wt` directly, e.g. `dir=$(git-wt path 1)`.

`--alias <name>` installs a shell function of that name into your rc
(`~/.zshrc`, `~/.bashrc`, or `~/.profile`) inside a managed block, refreshed on
reinstall.

## Usage

See [docs/COMMANDS.md](docs/COMMANDS.md) for every command, its options, and
sample output.

```sh
git-wt                # list worktrees
git-wt add feature/x  # create a worktree for feature/x, cd's in via `wt`
git-wt switch 2       # cd into worktree 2 (via `wt`)
git-wt merge 1,2      # merge worktree 2's branch into worktree 1's
git-wt --help         # options, no prose
git-wt --help -f      # full manual: every flag, every section
```

## Build

`build.sh` sets the version, compiles, and bundles one shareable installable
file:

```sh
./build.sh          # build at current version
./build.sh patch    # bump x.y.Z, then build
```

The release binary lands at `target/release/git-wt`, and the shareable file at
`dist/git-wt-<version>-<os>-<arch>.install.sh` — the same one-file installer
from [Install](#from-the-one-file-installer-no-rust-needed): gzipped binary
embedded, no repo or toolchain needed to run it.

`./test.sh` auto-detects the host OS and runs the test suite natively.
`./test.sh --docker` builds and runs the suite in a throwaway Debian
container instead; `./test.sh --build-install` also verifies the one-file
installer end-to-end on Linux, via Docker.
