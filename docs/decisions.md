# Decisions

Short ADR entries for decisions the specification does not cover.

## ADR-001: Workspace with two crates

Context: the test double for `bw` must be a separate executable, and the
application is a library plus a binary.

Decision: a Cargo workspace with `crates/lazybw` (lib + bin) and
`crates/fake-bw` (bin).

Consequences: `fake-bw` can be depended on as a dev-dependency to locate the
built binary; no feature juggling in a single crate.

## ADR-002: Single shared `AppState` in the TEA core

Context: the design asks for a pure `draw(&AppState, &Theme)` and a `update`
function returning effects.

Decision: all mutable UI state lives in `AppState`; ratatui widget states and
the animation manager live in a separate `UiState` owned by the render loop.
Effects are executed by an async executor that posts `Msg`s back.

Consequences: update logic is testable without a terminal; the render loop is
the only place that touches ratatui state mutably.

## ADR-003: `Sensitive` newtype, no `Display`

Context: secrets must never leak through `Debug`, `Display` or logs.

Decision: `bw::sensitive::Sensitive` is a newtype over `String` with
`ZeroizeOnDrop`, a redacted `Debug`, no `Display`, and a single `expose()`
accessor. Models store secrets in `Sensitive`. Serialisation is transparent so
the wire format is unchanged; `serde_json::Value` bodies are never logged.

Consequences: printing a secret requires an explicit `expose()` call, which is
easy to audit.

## ADR-004: One backend actor owns the connection and the session key

Context: `bw serve` is single-threaded and lazybw uses one socketpair.

Decision: a single tokio task owns the `SendRequest`, the session key and the
`bw serve` child. The UI sends commands with correlation ids and receives
`BackendEvent`s; the session key never leaves the task.

Consequences: requests are serialised by construction, the UI never blocks, and
a crash restart is local to the actor.

## ADR-005: Notes open in an embedded edtui editor

Context: the plan lists `edtui` for the notes field with full Vim modes.

Decision: `Enter` on the Notes row opens a dedicated notes view backed by
`edtui::EditorState`, so NORMAL/INSERT/VISUAL, motions and operators inside the
note are edtui's. `Tab`/`Shift-Tab` in NORMAL write the text back into the form
and move to the next field; `Ctrl-e` still opens `$EDITOR`; `Ctrl-s` saves;
`:` closes the notes view and opens the lazybw command line. The edtui handler
is constructed fresh per key press so it never holds a second Vim state.

Consequences: one editing stack for notes with its own modal semantics, and
the form remains the single place where the item is assembled and validated.

## ADR-006: Folder filters include the subtree

Context: the spec distinguishes real folders from virtual parents but does not
say what selecting a parent shows.

Decision: selecting a folder shows the items in that folder **and** its
subfolders; counts include descendants. Virtual nodes therefore behave like
real ones for filtering, and cannot be renamed, deleted or used as a move
target.

Consequences: the sidebar count of a parent matches what the list shows.

## ADR-007: Motion lives in `ui/fx.rs`, mouse in the update function

Context: the design asks for the effects of design.md §7 and for mouse support.

Decision: `ui/fx.rs` implements every listed effect: popup fade-in/out and the
55% backdrop, toast sweep, trash dissolve and copy flash, TOTP coalesce,
master-password shake, unlock sweep and tab fade. Screen fades are tachyonfx
effects driven by an `EffectManager`; shake, flash and toast offsets are small
time-based state machines. Everything is gated by `ui.animations` (and
`NO_COLOR`) and disabled in tests and snapshots. Mouse events click to focus or
select in the sidebar/list/detail, click the tabs, scroll the focused pane and
drag the two pane dividers; widths live in the session state only.

Consequences: motion never delays input and cannot affect tests; mouse is a
convenience, keyboard remains complete.

## ADR-008: `fake-bw serve` reloads the state file per request

Context: import and export run as short-lived `bw` subprocesses that update the
same state file while `serve` holds it in memory.

Decision: `fake-bw serve` reloads the JSON state before handling every request
and writes it back after mutations.

Consequences: tests observe subprocess imports without restarting serve, which
mirrors the real CLI's `data.json` sharing closely enough for the contract.

## ADR-009: Integration tests serialise environment mutations

Context: `fake-bw` selects its state file with `$FAKE_BW_STATE`, a process-wide
variable, and Rust 2024 makes `set_var` unsafe.

Decision: the test harness holds a global mutex while a test app is alive and
sets/removes the variables around it. Each boot uses a fresh temp directory.

Consequences: tests are deterministic; they run sequentially (about 2 s total).

## ADR-010: `:gen` opens the generator

Context: `docs/keymap.md` specifies `:gen [length]` / `:gen phrase [words]` as
"generate with these options and copy".

Decision: `:gen` opens the generator tab, where length, mode and copy are one
keypress away. The parser still accepts and ignores the optional arguments.

Consequences: no hidden copy from the command line; the generated value is
visible before it reaches the clipboard.


## ADR-011: Subfolder-aware rename and delete

Context: the spec asks rename to offer "Also rename N subfolders" (default on)
and delete to offer "Also delete N subfolders" (default off).

Decision: both operations open the shared confirmation dialog with a checkbox
(`space`/`x` toggles). Rename applies `old/…` → `new/…` to every descendant and
records one undo entry; delete removes the descendants first, then the parent.
The delete confirmation message states the item count that moves to No folder.

Consequences: a folder operation is one user action but several `PUT`/`DELETE`
requests, all tracked by the same pending flag; the folder list reloads once
per response.


## ADR-012: Embedded assets live inside the crate

Context: rustc's `include_str!` resolves relative to the source file, and
`cargo package` only packs files under the crate root. The plan's layout puts
`assets/` at the workspace root, which makes a published crate unbuildable.

Decision: the default config, themes and generated screenshots live in
`crates/lazybw/assets/`. `README.md` and `docs/configuration.md` point there.
`Cargo.toml` excludes stale `.snap.new` files from the package, and the release
checklist verifies the tarball contains the embedded assets.

Consequences: `cargo package` and `cargo install --path crates/lazybw` are
self-contained; the workspace root no longer has an `assets/` directory.


## ADR-013: Folder delete undo recreates by name

Context: folder ids change when a deleted folder is recreated, and the items
that lived there must find their way back.

Decision: a `FolderDelete` undo entry records each removed folder's path and
the ids of its items. Undo creates the paths parents-first (so nested names
remain valid), maps each new id by name, then PUTs every item with that new
folder id and reloads. Redo resolves the same paths by name, deepest first, and
deletes them again. Rename entries store old/new paths and resolve the current
id at execution time, so they survive recreation.

Consequences: undo and redo both work after ids change; names are the stable
identity across the operation.


## ADR-014: Health accepts both the list envelope and a bare array

Context: `docs/bw-integration.md` described the password-health payload as a
bare array, but the real CLI 2026.10.1 returns
`{"object":"list","data":[…]}`, which made the app fail with "invalid type:
map, expected a sequence".

Decision: `parse_health_payload` accepts the object envelope and, for older
builds and the test double, a bare array. The bundled CLI source is the
authority; the test double now emits the real shape and a unit test covers
both.

Consequences: health works against the current CLI without dropping
compatibility with older builds. Validated live against `bw` 2026.10.1 on
2026-10-31: the Health tab reported 156 logins, the 0–100 score, all five
category counts and HIBP exposure counts.


## ADR-015: A pure prefix keeps which-key open past `timeoutlen`

Context: `timeoutlen` (1000 ms) reset every pending sequence, so the leader
popup appeared at 300 ms and vanished 700 ms later, too fast to read.

Decision: `timeoutlen` only resolves a pending sequence that is itself a
complete binding (Vim's ambiguous-mapping case). A pure prefix such as
`<Space>` waits for the next key or `Esc`, like which-key.nvim. Group labels
from `docs/keymap.md` §5 are built in and looked up by the full prefix.

Consequences: which-key stays up until the user acts; `Esc` closes it at once.


## ADR-016: Master password re-prompt is consumed by one action

Context: after a successful re-prompt the original action ran through the
same check and asked again, forever; several paths (reveal, notes, quick open,
JSON edit, password history, `gp`) skipped the check entirely.

Decision: every secret-revealing or editing path calls `reprompt_required`. A
successful `POST /unlock` marks the item for exactly the action that asked
(`reprompt_once`); `security.reprompt_grace` extends that to a time window. A
wrong password stays in the dialog with the CLI message. The export wizard
verifies its master password the same way before writing anything.

Consequences: re-prompted items are usable and nothing bypasses the prompt.
`POST /unlock` refreshes the session key, which the backend already tracks.


## ADR-017: Typed passwords live in `SecretInput`

Context: the unlock, re-prompt, export, import and API-key inputs were plain
`String`s with derived `Debug`, never wiped.

Decision: `input::secret::SecretInput` pre-allocates, wipes old allocations
when it grows or shrinks, redacts `Debug`, zeroes on drop and converts to
`SecretString` with `take_secret`. Paste fills these fields with the first
line only. `$EDITOR` runs get flags that disable swap, undo, backup and
history files for vi/vim/nvim, emacs and micro.

Consequences: transient secrets no longer linger in freed memory or editor
side files for the common editors; other editors are run unchanged.


## ADR-018: `:import FORMAT PATH` opens at the file step; filtered imports keep folders

Context: the format step could not change the format, previews were attempted
for CSV, the duplicate-free copy dropped every folder and was never deleted,
and `bw serve` did not see imported items until a manual sync.

Decision: the format step is a filterable list (Bitwarden formats first, then
`bw import --formats`); arguments on the command line skip it. Only
`bitwardenjson` gets the preview. The filtered copy keeps the file's folders,
is written to the private runtime directory and is shredded by the backend
after `bw import`. A forced sync follows every import, then All items sorted
by created date is shown.

Consequences: imports keep their folder structure and leave no plaintext copy
behind.

## ADR-019: Answers are tied to the request and the session that sent them

Context: lock cleared the interface at once, but a vault load already in
flight could still arrive and put the decrypted cache back. A save answer
updated whichever editor was open, even for another item, and replaced edits
made after the save was sent. Background syncs moved the user off the
editor. Optimistic changes (trash, favourite, move) kept their undo entry
when the server refused them.

Decision: locking forgets every pending data request; an answer that carries
vault data and matches no pending request is dropped, and a snapshot is only
accepted while loading or unlocked. Save requests record whether the editor
sent them and the form revision at that moment: the answer only touches that
editor, and if the form changed since, it keeps the user's text and takes
only the id and revision from the server. Refreshes never change the screen
after the first load. A failed optimistic change drops its undo entry and
reloads the vault. Saving from the editor records one undo step.

Consequences: the interface never shows vault data after a lock, and an
answer cannot overwrite work it did not see. A failure costs one reload.

## ADR-020: A clipboard that cannot be read back is cleared anyway

Context: the timed clear only cleared when it could read the clipboard and
confirm it still held lazybw's value. Over OSC 52 nothing can be read back,
so the copied password simply stayed.

Decision: if the clipboard can be read and holds something else, it is left
alone; if it holds lazybw's value, or cannot be read at all, it is cleared.
A failed clear is retried a few times. The last copy is kept in a zeroizing
buffer.

Consequences: over SSH a copy made by the user after lazybw's may be cleared
too. That is the lesser risk, and it is documented in the security chapter.

## ADR-021: Speed comes from not repeating work, not from caching secrets

Context: with 5000 items, each sync parsed the whole response twice (once
into a JSON tree only to read `success`), the detail pane ran zxcvbn and
copied the item's secrets on every frame, the brand icon parsed a URL per
row per frame, and the health screen sorted its lists twice per frame.
Several further ideas were measured: per-token score caches and incremental
narrowing for search, a hand-written date parser, a hashed folder lookup, a
cached TOTP generator.

Decision: the envelope is read with `IgnoredAny` and the body parsed once;
the detail pane borrows the item instead of cloning it; the registrable
domain is kept on the row; the health report stores each category's order
when it is built; folder counts and the folder filter work on slices. The
password strength of the last password measured is cached under a hash with
a random per-process key, and forgotten on lock. Search stays a full scan
with `nucleo`: at about 1.6 ms per key for 5000 items it is far inside a
frame, and the caches that would make it faster must be invalidated on every
vault change or depend on `nucleo`'s internal scoring. The date parser, the
hashed folder lookup and the TOTP cache were not taken: their gains were
within noise or below a microsecond, and the TOTP cache would keep a parsed
secret around.

Consequences: a sync costs half as much, drawing an item about 2.3 times
less, and fewer copies of secrets are made. `examples/bench.rs` covers these
paths so a regression shows up.

