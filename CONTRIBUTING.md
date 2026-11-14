# Contributing to lazybw

Bug reports, ideas and pull requests are welcome. Questions go to
[Discussions](https://github.com/SyscallBrain/lazybw/discussions). For
security problems, please follow [SECURITY.md](SECURITY.md) instead of
opening an issue.

## Setting up

You need Rust 1.88 or newer. The Bitwarden CLI is not needed for
development: the tests run against `crates/fake-bw`, a stand-in that keeps
its vault in a JSON file, and never touch a real vault.

```sh
git clone https://github.com/SyscallBrain/lazybw
cd lazybw
cargo run            # needs a real `bw` to log in
scripts/check.sh     # format, lints and every test, as CI runs them
```

## Making a change

1. Read [ARCHITECTURE.md](ARCHITECTURE.md) for where things live.
2. Keep the change focused; one pull request per fix or feature.
3. Add or update tests. Interface changes come with updated snapshots:
   review them with `cargo insta review` (or run the tests with
   `INSTA_UPDATE=always` and check the diff).
4. If you change a key binding, an option or a command, update
   `book/src/reference/keymap.md` or `book/src/reference/config.md` (tests
   check the default config matches) and the relevant chapter in `book/src/`.
5. Run `scripts/check.sh` before pushing.

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org)
(`fix:`, `feat:`, `docs:` …) and say why the change is needed.

## Adding an action

An action is anything a key can do.

1. Register it in `crates/lazybw/src/app/actions.rs` with a description and
   the contexts where it applies.
2. Handle it in `crates/lazybw/src/app/update/` (the file for its area).
3. Give it a default key in `crates/lazybw/src/config/keymap.rs`, if it
   should have one, and list it in `book/src/reference/keymap.md`.
4. Test it through the harness in `crates/lazybw/tests/` (`app.keys("…")`).

## Performance

`cargo run --release --example bench` times the paths that run on every
key or frame (search, drawing, sync, health) on a 5000-item vault. Use
`--filter` to run some cases and `--json` to compare two builds; run the
two builds in turn, a few rounds each, so that load on the machine affects
both. A change that makes something faster should show it there, and should
not make the rest slower.

## Rules that keep secrets safe

- Never put a secret in process arguments, logs, panics or `Debug` output.
  Decrypted values use `Sensitive`; typed passwords use `SecretInput`.
- Files that may hold secrets are written with mode 0600 through
  `util::atomic::write_atomic`; temporary files go in the private runtime
  directory and are shredded.
- `update` never blocks or touches the terminal; it returns effects.

The [contributing chapter](https://syscallbrain.github.io/lazybw/contributing.html)
of the user guide has more detail.
