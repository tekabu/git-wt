## Review: current changes (negatives only)

### 1. Integration tests are broken
`test-mac.sh` currently reports **378 passed, 8 failed**. Failing cases:

- `show alias works` / `legacy show order now works` — the `show` alias for `path` is not implemented, so `git-wt show 1` exits with “no worktree named 'show'”.
- `merge self refused` — `git-wt merge 1,1` now emits the generic duplicate error `worktree #1 listed twice` instead of the specific `already checked out in worktree 1`.
- `merged current in itself`, `merged branch is in cb2`, `merged list form reversed`, `merged detached list form` — success messages for `merged` were moved from **stderr to stdout** (`src/cmd/merged/mod.rs:88` uses `println!`), breaking the documented stdout contract and these assertions.
- `merged 2 self-check` — hardcodes worktree `#2` as `feat-a`, but the actual ordering lands on `cb1`; the test should resolve the number dynamically like other cases.

### 2. Documentation claims features the code does not implement
- `docs/COMMANDS.md` and `docs/MANUAL.md` list `path` alias **`show`**; `src/cli.rs:65` (`Path` subcommand) has no alias.
- `docs/MANUAL.md:712` claims `--ot` is an alias for `--others`; `src/cmd/merged/args.rs:19` only defines `-o, --others`. Running `git-wt merged --ot` fails.
- `_alias.sh:70` treats `show` as a pass-through verb, so `wt show 1` will also fail at the binary.

### 3. Clap migration is incomplete
- `commits`, `log`, `merge`, `diff`, and `sync` still hand-roll their own flag parsers (`src/cmd/commits/args.rs`, `src/cmd/merge/mod.rs`, `src/cmd/diff/mod.rs`, `src/cmd/sync/mod.rs`) despite the plan stating flags would be parsed by clap derive. This leaves two parallel parsing systems in the codebase.
- `git-wt --help` does not display command aliases (`a`, `ls`, `cd`, `s`, `m`, etc.) because clap’s `alias` attribute is hidden by default; use `visible_alias` if you want users to discover them.

### 4. Code quality / new lint debt
`cargo clippy -- -D warnings` fails. Newly introduced warnings include:
- `match_result_ok` — `src/cmd/switch/mod.rs:45`.
- `cloned_ref_to_slice_refs` — `src/main.rs:259` and `src/main.rs:266`.
- `type_complexity` — `src/main.rs:461`.
- `src/cli.rs:13` imports `Worktree` with unnecessary braces.

### 5. Naming and structure issues
- `src/cmd/list/mod.rs:365` defines `cmd_switch`, but it is the interactive **picker**, not the switch command. `src/cmd/switch/mod.rs:6` imports it as `cmd_picker`, which confirms the name is misleading.
- `src/main.rs` still contains a ~350-line command dispatch match; the refactor did not materially shrink the dispatcher.
- `src/main.rs:38-46` uses an unsafe `signal` binding with an incorrect C signature (`handler: usize`) to restore SIGPIPE. This is non-portable and undefined-behavior-prone.

### 6. Behavioral / contract regressions
- `cmd_merged` success output on stdout breaks the documented rule that only `switch`/`path`/`add`/`remove` print paths to stdout (`docs/MANUAL.md:779`).
- `merge 1,1` loses its specific self-check message because the centralized duplicate-target check in `src/main.rs:488-510` runs before command-specific validation.

### 7. Test coverage gaps
- `test-linux.sh` was not touched at all; it will now run the updated `test-mac.sh` and inherit the same failures.
- `docs/baseline-tests.txt` is stale relative to the new grammar.
- `docs/test-report.md` was committed containing the 8 failing cases.