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
the environment or a PTY, never through arguments. The long ones (the health
report, imports, exports) run beside the actor's queue, so a lock is never
stuck behind them; a lock or a logout stops the health report and an export
(ADR-022). The queue to the actor has no limit: the interface sends from its
own loop and must never wait there.

## State machines

Where the next step depends on where the user is, the state is an enum in
`app/state.rs`. Data that only makes sense in one situation lives inside
that variant, so an impossible combination does not compile.
`tests/state_machines.rs` drives each machine through `update`.

| Machine | Type | Changed by |
|---|---|---|
| Session | `SessionPhase` | `AppState::enter_session` only |
| Popup over the screen | `Option<Overlay>` | `AppState::open`, `take_picker()` and the other `take_*` |
| Export wizard | `ExportStep` | `export_key` in `update/wizards.rs` |
| Import wizard | `ImportStep` (the preview step holds the preview) | `import_key` in `update/wizards.rs` |
| Login form | `LoginFocus`, `LoginServer`, `LoginMethod` | `login_action` in `update/overlays.rs` |
| Master password re-prompt | `RepromptState` | `reprompt_key`, and the `Unlocked` answer in `update/backend.rs` |
| Key sequences | `Dispatcher` | `input/dispatcher.rs` |

A few rules come with them:

- Each session phase has its own screen, and leaving Loading or Ready
  forgets the decrypted vault. A test fails if code outside `state.rs`
  assigns `state.session` or a session screen directly. The machine is
  drawn in the doc comment of `Session` in `app/state.rs`.
- At most one overlay is open. It receives every key, leads the keymap
  context chain, closes on `Esc`, and takes a paste or drops it; nothing
  reaches the screen behind it.
- The re-prompt dialog keeps the id of the request checking its password.
  Only that answer can run the dialog's action, so closing the dialog
  cancels it.
- Busy indicators are not stored. `AppState::awaiting` and helpers such as
  `unlocking()` and `exporting()` look at the requests in flight, so an
  indicator ends when its request does, however it ends.

Smaller modes are plain fields and are fine that way: the editor's INSERT
mode (`EditorState::insert`), visual mode (`visual_anchor`), search
(`SearchState::active`), a divider drag (`drag`) and a key capture in the
keymap editor (`KeymapEditorState::capture`).

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
- `tests/state_machines.rs` walks the transitions of each state machine
  above, including the ones that must not happen.
- `tests/snapshots.rs` renders every screen with `TestBackend` (insta).
- `tests/security.rs` and `tests/logs.rs` check that secrets stay out of
  arguments, logs and the clipboard.
