# Contributing

Bug reports, ideas and pull requests are welcome on
[GitHub](https://github.com/SyscallBrain/lazybw). Questions go to
[Discussions](https://github.com/SyscallBrain/lazybw/discussions).

## Building and testing

```sh
git clone https://github.com/SyscallBrain/lazybw
cd lazybw
cargo build
scripts/check.sh
```

`scripts/check.sh` runs `cargo fmt --check`, Clippy with warnings as errors
and the whole test suite. A pull request should pass it.

The tests never touch a real vault. `crates/fake-bw` is a stand-in for the
Bitwarden CLI that keeps its vault in a JSON file and records every request,
so tests can check exactly what lazybw sent. The screens are covered by
snapshot tests with [insta](https://insta.rs): after an intended change to the
interface, review and accept the new snapshots with `cargo insta review`, or
run the tests with `INSTA_UPDATE=always`.

## How the code is organised

lazybw follows The Elm Architecture. All state lives in one `AppState`.
Input, timers and answers from `bw` become messages; a pure `update` function
changes the state and returns effects (send a request to `bw`, copy to the
clipboard, open `$EDITOR`), and the main loop carries the effects out.
Drawing reads the state and changes nothing.

| Path | Holds |
|---|---|
| `crates/lazybw/src/app/` | state, messages, effects, and `update/` split by area |
| `crates/lazybw/src/ui/` | drawing: `widgets.rs` and one file per group of screens in `screens/` |
| `crates/lazybw/src/bw/` | finding `bw`, starting `bw serve`, the HTTP client and the models |
| `crates/lazybw/src/vault/` | the item cache, filters, folders, search and health checks |
| `crates/lazybw/src/input/` | the key dispatcher, line editing and secret input |
| `crates/lazybw/src/config/` | configuration, keymap and settings |
| `crates/lazybw/src/theme/` | themes and colour handling |

The `docs/` folder has the longer design notes: `design.md` for the screens,
`keymap.md` for the keymap model, `bw-integration.md` for everything about
talking to `bw`, and `decisions.md` for the reasons behind the main choices.

## This book

The book lives in `book/` and is built with [mdBook](https://rust-lang.github.io/mdBook/):

```sh
cargo install mdbook
mdbook serve book --open
```

The screenshots in it come from `cargo run -p lazybw --example screenshots`,
and the logo from `python3 scripts/make-logo.py`.
