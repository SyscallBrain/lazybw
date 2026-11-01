# Architecture

lazybw is a terminal front end for the official Bitwarden CLI. It never
decrypts anything itself and never opens a network connection: it starts
`bw serve`, talks to it over a private socket pair, and draws the result.

```text
 terminal ──keys──▶ input thread ──Msg──▶ update(&mut AppState, Msg) ──▶ Vec<Effect>
    ▲                                            │                          │
    └──────── draw(&AppState) ◀── dirty ─────────┘          App::run_effects │
                                                                             ▼
                                   BwService actor ◀── CommandKind ── Effect::Backend
                                         │
                              HTTP/1.1 over socketpair (fd 3)
                                         ▼
                                     bw serve (Node.js, official CLI)
```

## The Elm Architecture

Everything the interface shows lives in one `AppState`
(`crates/lazybw/src/app/state.rs`). Keys, mouse, paste, timers and answers
from `bw` arrive as `Msg` values (`app/msg.rs`).

`app::update::update` changes the state and returns `Effect`s: send a
command to `bw`, copy to the clipboard, open `$EDITOR`, write a file. It
never blocks and never touches the terminal, which is what makes it testable
without one. `App::run_effects` (`app/mod.rs`) carries the effects out; the
ones that need the terminal (`$EDITOR`, `bw login`) are run by the main loop
in `cli.rs`, which hands the terminal over and takes it back.

`ui::draw` reads the state and changes nothing.

## The backend

`bw/service.rs` is an actor that owns the `bw serve` child and its HTTP
client. Requests carry a correlation id; answers come back as
`BwEvent`s, and `app/update/backend.rs` matches them to the pending request.
Locking forgets pending data requests, so an answer that arrives after a lock
is dropped. Short tasks that `bw serve` cannot do (login, export, import) run
as separate `bw` processes (`bw/subprocess.rs`), with secrets passed through
the environment or a PTY, never through arguments.

## Where things live

| Path | Holds |
|---|---|
| `crates/lazybw/src/cli.rs` | subcommands, terminal setup, the main loop |
| `crates/lazybw/src/app/` | state, messages, effects, actions, and `update/` split by area |
| `crates/lazybw/src/ui/` | drawing: `widgets.rs` and one file per group of screens in `screens/` |
| `crates/lazybw/src/bw/` | locating `bw`, `bw serve`, the HTTP client, models, `Sensitive` |
| `crates/lazybw/src/vault/` | item cache, filters, folders, search, health, duplicates |
| `crates/lazybw/src/input/` | key dispatcher, line editing, `SecretInput` |
| `crates/lazybw/src/config/` | configuration, durations, keymap, persistence |
| `crates/lazybw/src/theme/` | themes and colour resolution |
| `crates/lazybw/src/security.rs` | process hardening, private temporary files, shredding |
| `crates/fake-bw/` | the test double for the Bitwarden CLI |
| `book/` | the user guide (mdBook) |
| `docs/` | design notes: `design.md`, `keymap.md`, `bw-integration.md`, `decisions.md` |

## Testing

- `crates/lazybw/tests/flows.rs` drives the real app against `fake-bw` with
  key sequences.
- `tests/regressions.rs` drives `update` directly to check what must not
  happen (stale answers, links opened without asking).
- `tests/snapshots.rs` renders every screen with `TestBackend` (insta).
- `tests/security.rs` and `tests/logs.rs` check that secrets stay out of
  arguments, logs and the clipboard.

## Decisions

The reasons behind the main choices are recorded in
[docs/decisions.md](docs/decisions.md).
