# git-wt — Cross-Platform Issues (Linux, macOS, Windows)

Status: audit of current `main` against Linux, macOS, and native Windows usage.

The Rust binary is intentionally small and mostly portable. The main gaps are in path-separator handling, filename sanitization, and the Unix-centric shell scripts.

---

## High priority — functional bugs on Windows

### 1. `test.sh` hardcodes `/tmp`

**File:** [`test.sh`](../test.sh#L27)

```bash
ROOT="$(mktemp -d "/tmp/git-wt-test.XXXXXX")"
```

**Problem:**
- `/tmp` does not exist on native Windows.
- Even on Linux/macOS, this ignores `$TMPDIR`.

**Impact:** Tests fail or error out on Windows before they start.

**Suggested fix:**
```bash
ROOT="$(mktemp -d)"
# or
ROOT="$(mktemp -d "${TMPDIR:-/tmp}/git-wt-test.XXXXXX")"
```

---

### 2. `--dirname` only detects `/` as a path separator

**File:** [`src/main.rs`](../src/main.rs#L560)

```rust
if d.contains('/') {
```

**Problem:**
- On Windows a user naturally writes `--dirname sub\test` or `--dirname sub/test`.
- The code only recognizes `/`, so `sub\test` is treated as a leaf and the `\` is sanitized to `-`, producing `sub-test` instead of a nested path.
- Behavior is inconsistent across platforms.

**Suggested fix:**
```rust
use std::path::Path;

if Path::new(d).components().count() > 1 {
    // d is a path (relative or absolute)
}
```

This works for both `sub/test` and `sub\test` on any platform.

---

### 3. `sanitize()` does not remove Windows-invalid filename characters

**File:** [`src/main.rs`](../src/main.rs#L692)

```rust
let c = if matches!(c, '/' | ' ' | ':' | '\') { '-' } else { c };
```

**Problem:**
- Windows filenames cannot contain `* ? " < > |`.
- Branch names that include these characters will create an invalid directory name and `git worktree add` will fail with a confusing error.

**Suggested fix:** extend the set of characters collapsed to `-`:

```rust
let c = if matches!(c, '/' | ' ' | ':' | '\\' | '*' | '?' | '"' | '<' | '>' | '|') {
    '-'
} else {
    c
};
```

---

## Medium priority — scripts do not run on native Windows

### 4. `install.sh` is POSIX/Bash only

**File:** [`install.sh`](../install.sh)

**Problem:**
- Uses `$HOME/.cargo` (Windows normally uses `%USERPROFILE%\.cargo`; `$HOME` is not guaranteed outside WSL/Git Bash).
- Detects the shell via `$SHELL` and writes to `~/.zshrc`, `~/.bashrc`, or `~/.profile`.
- Depends on `touch`, `grep`, `awk`, `sed`, `mktemp`, `mv`.

**Impact:**
- Native Windows/PowerShell users cannot run the installer.
- WSL/Git Bash users can use it, but the resulting alias is only useful in that environment.

**Suggested fix:**
- Keep `install.sh` for Unix-like environments.
- Add an `install.ps1` PowerShell script for Windows, or document that Windows users should run:
  ```powershell
  cargo install --path .
  ```
  and add their own alias/function manually.

---

### 5. `build.sh` depends on GNU/BSD utilities

**File:** [`build.sh`](../build.sh)

**Problem:**
- Uses `grep -m1` and `sed -E`.
- Uses `awk` for the version rewrite.
- These tools are not available on native Windows.

**Impact:** Native Windows users cannot bump the version or run the scripted build.

**Suggested fix:**
- Document that Windows users can do the equivalent manually:
  ```powershell
  # Edit Cargo.toml version, then:
  cargo build --release
  ```
- Or provide a PowerShell equivalent (`build.ps1`).

---

## Low priority — cosmetic

### 6. ANSI color codes in `test.sh`

**File:** [`test.sh`](../test.sh#L90-L93)

```bash
printf '  \033[32mPASS\033[0m  %s\n' "$name"
printf '  \033[31mFAIL\033[0m  %s  (%s)\n' "$name" "${why# ; }"
```

**Problem:**
- Modern Windows Terminal handles ANSI escapes fine, but legacy `cmd.exe` requires explicit ANSI enablement.
- No functional impact; output just shows raw escape codes on old consoles.

**Suggested fix:** Optional — leave as-is and rely on modern terminals.

---

## Summary table

| Component | Linux | macOS | Native Windows | Notes |
|---|---|---|---|---|
| `src/main.rs` (binary) | ✅ | ✅ | ⚠️ | `--dirname` separator detection and `sanitize` need Windows fixes |
| `install.sh` | ✅ | ✅ | ❌ | Needs PowerShell alternative or documentation |
| `build.sh` | ✅ | ✅ | ❌ | Use `cargo` directly on Windows |
| `test.sh` | ✅ | ✅ | ❌ | `/tmp` hardcoded; needs portable temp dir |
| ANSI colors in tests | ✅ | ✅ | ⚠️ | Cosmetic on legacy Windows consoles |

---

## Recommended order of fixes

1. Fix `--dirname` path detection in `src/main.rs` using `Path::components`.
2. Extend `sanitize()` to collapse Windows-invalid filename characters.
3. Make `test.sh` use a portable temp directory.
4. Add a Windows PowerShell installer (`install.ps1`) or update the README with Windows manual install steps.
5. Add a PowerShell build helper (`build.ps1`) or document the manual Windows build flow.

After fixes 1–3, the core Rust binary and the test suite should be fully usable on Linux, macOS, and Windows.
