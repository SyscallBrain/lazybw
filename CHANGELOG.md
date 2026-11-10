# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `lazybw doctor` shows the lazybw version, so its output is enough to
  tell which release a bug report is about.
- `:gen 32` and `:gen phrase 6` generate a password or passphrase and copy
  it, and `:sort name!` sorts in reverse, as the guide already said.
- The command line keeps its history between sessions; `Up` and `Down` go
  through it, showing only the commands that start with what is typed.
- `Ctrl-w` deletes the word before the cursor and `Ctrl-u` everything before
  it in the command line, search, quick open, prompts, the wizards' fields and
  the editor. In password fields `Ctrl-u` clears what was typed.
- The command line, prompts and search show where the cursor is.
- The export wizard says when the file exists and replaces it only on a
  second `Enter`. The suggested file name follows the format.
- A card number that fails the Luhn check is saved with a warning.
- "Editor command" can be changed in the settings.

### Changed

- Secure notes show their line count in the list, not their first line
  (ADR-023). The text stays in the detail pane.
- The tabs are not shown before the vault is open.
- The statusline says "locks at 14:15", and on a narrow terminal drops the
  last-sync time and the lock time before the position.
- A theme picked from the theme list (or the settings) is kept for the next
  start. `:theme` still tries one for the session.
- The generator's session history keeps the passwords `r` made, not one per
  option change.
- `n` in a dialog, `Esc` in INSERT and the rest of the `confirm` and `insert`
  contexts follow the keymap, so they can be rebound like any other key.

### Fixed

- lazybw now writes the log that `lazybw doctor`, the troubleshooting guide
  and the bug report form point to (`~/.local/state/lazybw/lazybw.log`).
  It was never written before. It records start-up, the `bw` that was found
  and errors; `LAZYBW_LOG=debug` adds the error output of `bw`. The file is
  readable only by you, and when it passes 1 MiB it is moved to
  `lazybw.log.1` at the next start.
- Verification codes work for authenticator keys of 16 characters (80
  bits), which many sites issue. lazybw refused them as too short, because
  the TOTP library asks for 128 bits; keys under 80 bits are still refused.
- An SSH private key with a passphrase is refused with "private key is
  encrypted" when it is imported or pasted into an SSH key item. It used to
  be stored still encrypted, where nothing could use it.
- An action on many items at once (a few hundred marked or selected, or a
  large trash being emptied) no longer freezes lazybw; past roughly 800 items
  it used to hang for good, with only part of the change sent.
- `q` waits for changes that are still on their way to Bitwarden. Pressed
  right after trashing, moving or saving, it used to stop the CLI before the
  change was sent, although the list already showed it as done. The message
  line says how many changes are left; `Esc` takes the quit back and `:q!`
  leaves at once.
- Marks and the visual selection end with the action that used them. Marks
  used to stay, so the next `dd` acted on the old items again instead of the
  one under the cursor, and a second action after trashing a visual selection
  at the end of the list crashed lazybw.
- "Delete permanently?" says how many items it will delete, and deletes
  exactly those: it used to name the first one only, and to delete whatever
  was selected by the time it was answered. Removing a passkey asks once, not
  twice.
- The mouse does nothing behind a dialog, on the lock and login screens, or
  under the editor. A click on the list behind "Delete permanently?" used to
  change what `y` deleted.
- `:move` to a folder that does not exist says so and moves nothing; it used
  to take the item out of its folder. Folder names, field names and paths
  with spaces work on the command line (`:move Social media`,
  `:folder new Side projects`, `:copy field Recovery codes`), and `Tab`
  completes them.
- The server chosen on the login screen is the one lazybw logs in to.
  Choosing bitwarden.com or bitwarden.eu used to change nothing: the login
  went to whichever server `bw` had been set to before.
- After `$EDITOR` or `bw login`, pasted text is a paste again (it arrived as
  typed keys and ran as commands) and `hide_on_focus_lost` works again.
- A crash gives the terminal back whole. Mouse reporting, bracketed paste and
  the keyboard protocol used to stay on in the shell afterwards.
- `Ctrl-f` pages down. The default binding never worked: `<C-f>` was read
  as a function key without a number and dropped.
- `d` in visual mode acts at once; it waited a second for a `dd` that only
  exists outside visual mode.
- The page motions (`Ctrl-d`, `Ctrl-f`, `H`, `M`, `L`) move by the height
  of the list on screen instead of 10 and 20 rows, `zz`, `zt` and `zb`
  scroll the list, and help scrolls by pages and stops at its end.
- The mouse wheel scrolls the pane under the pointer; it always scrolled the
  sidebar. Dragging a pane divider resizes the panes.
- Locking (and quitting) clears the clipboard only if it still holds what
  lazybw copied; it used to wipe whatever was there.
- The automatic lock counts time spent asleep: a laptop closed for the night
  came back unlocked.
- A lock is no longer stuck behind a running health check, import or export:
  `bw serve` locks at once, and the health check or export stops (ADR-022).
  A password-protected export stops too, and its unfinished file is removed.
- `yy` on the verification code of an item that asks for the master password
  asks for it, as `yt` does.
- Turning off `health.check_exposed` takes effect at once instead of at the
  next start.
- The filtered copy of an import is always shredded, also when the vault was
  locked first; files left by a crashed lazybw are removed at the next start.
- `Ctrl-s` in the notes editor saves the notes too; it saved the item
  without them. Pasting into the notes editor works, and pasted line breaks
  are kept in notes.
- Saving a new item twice quickly creates it once.
- `:e!` loads the item again into the editor; it fetched it and left the form
  as it was.
- `gE` from the list opens the edited item in the editor, where it can be
  saved; the edit was thrown away. From the editor, `gE` edits the form as it
  is, not the saved item.
- A quick-open jump to an item hidden by the current search clears the
  search instead of landing on another item.
- Renaming or deleting a folder changes every folder with that name.
  Bitwarden allows two folders with the same name and the sidebar shows them
  as one; only the first was renamed or deleted, and undo did the same. The
  folder pickers list such a name once (ADR-025).
- The editor saves what it shows. The form kept its own copy of every value
  next to the item and copied it across at certain moments: a website added
  and then left was dropped from the item at the next key, and a value set
  from `$EDITOR` reached the item only when the form was next synced. The
  item is now the form's only copy, and typing in a password field edits it
  in place instead of copying it at every key.
- `gt`, `?` and other screens no longer leave an editor with unsaved changes
  behind, where the next `e` replaced it; help, settings and messages return
  to the editor.
- `SIGUSR1` while logged out does nothing; it showed the unlock screen.
- `i` and `cc` on a detail field open the editor on that field; they opened
  it on the name. `cc` empties the field.
- `:set` refuses values that do not fit (`:set mouse=maybe`) instead of
  writing a config that no longer loads, and accepts `on` and `off`.
- `Backspace` in the command line deletes before the cursor, not at the end.
  `Tab` completions no longer pile up names with spaces.
- `Esc` closes the import wizard while it runs; the import goes on.
- Moving past "Self-hosted" on the login screen keeps `j` and `k` as moves;
  `Enter` opens the URL field.
- The health score no longer says "would be 85" when it is 85, and the
  detail pane no longer shows "created" without a date.

## [1.1.0] - 2026-11-09

### Added

- The master password can be typed while "Starting Bitwarden CLI…" is on
  screen. `Enter` unlocks as soon as `bw` is ready, instead of waiting for
  the unlock screen first.

### Changed

- Opening the vault after unlocking takes about a tenth of a second less:
  lazybw no longer asks `bw` for its status after the unlock and after the
  vault loads, where it learnt nothing new.
- Search keeps up better in large vaults. Typing one more letter only
  re-checks the items that already matched, which halves the work per key
  with 5,000 items. The health screen draws only the rows on screen, and
  the local password check scores a reused password once instead of once
  per item.
- On the login screen, the API key form opens with `Enter` on "API key"
  and closes with `Esc`, and it marks the field being typed.
- The export and import wizards dim the screen behind them, like the other
  popups.
- "Bitwarden CLI not found" no longer offers a retry that could not work:
  lazybw looks for `bw` once, when it starts.

### Fixed

- `Space` opened the which-key panel anywhere from half a second to well
  over a second after the key, instead of after `keys.which_key_delay`
  (300 ms). An ambiguous key sequence likewise waited up to a second longer
  than `keys.timeoutlen` to run.
- A new item saved with `Ctrl-s` after using the password generator stayed
  marked as changed, and closing the editor asked to save it again. The
  same happened after picking a folder from the list.
- In the editor, `Tab` and `Shift-Tab` left INSERT mode, although the guide
  says they keep it. A form can now be filled in one go: `Tab` moves to the
  next field and typing carries on. Typing on a choice or a checkbox changes
  nothing.
- After unlocking, the panes took about two seconds to fill in, the right
  one last, instead of a quarter of a second. Animations only moved when
  something else redrew the screen, at worst once a second. The same delay
  hit toasts, the wrong-password shake and the flash on copy.
- A background sync, a manual sync or a folder change no longer sweeps the
  whole screen in again. That animation is for opening the vault.
- Typing `q` on the start-up screen quit lazybw, and whatever came next went
  to the shell. Only `Ctrl-c` quits there now.
- A wrong master password with `bw` 2026.10.1 showed "Bitwarden CLI error."
  instead of "Invalid master password." and the shake. That version answers
  with "Cryptography error, The decryption operation failed".
- The "terminal too small" message no longer cuts off the current size in a
  narrow terminal.
- Starting lazybw with no Bitwarden account logged in shows the login
  screen. It used to stay on "Starting Bitwarden CLI…" with an error.
- Cancelling a key sequence with `Esc` (for example `Space` then `Esc`)
  no longer leaves the keys shown in the statusline.
- The export wizard dropped `j` and `k` from everything typed into it. A
  file password containing them was silently shortened in both fields, so
  the backup was encrypted with a different password than the one typed.
  The password step also starts on the password field now.
- A paste while a picker, prompt or dialog is open no longer lands in the
  screen behind it. Prompts and picker filters take the paste.
- A configuration error at start-up stays on screen instead of being
  replaced by the login or unlock screen a moment later.
- A Bitwarden CLI that fails to start shows an error with a retry instead
  of an endless spinner.
- "You are not logged in" from the CLI now also clears the decrypted vault
  from memory.
- Cancelling the master password re-prompt while the password is being
  checked no longer runs the action anyway.
- After choosing "API key" on the login screen, another method can be
  chosen again, and `q` quits as the hint says.
- Pressing `Enter` twice no longer sends a second login or unlock.
- After unlocking, the vault opens straight away with a loading message,
  instead of an empty unlock screen followed by "No items".
- Messages say "1 item" rather than "1 items".
- With a `bw` that is too old or cannot be run, the start-up screen said
  "Bitwarden CLI not found" and asked you to install it. It now says what
  is wrong, and "Searched:" shows `bw.path` and `$LAZYBW_BW` when they are
  set instead of always "unset".

## [1.0.0] - 2026-11-01

First public release.

### Added

- Sessions through the official Bitwarden CLI (2025.9.0 or newer): server
  choice, email and password with two-step login, single sign-on, API key,
  unlock, lock (manual, on idle and on `SIGUSR1`), logout and periodic sync.
  `bw serve` runs on a private socket pair, with no port or socket file.
- Logins, secure notes, cards, identities and SSH keys: view, create, edit,
  clone, favourite, move, trash, restore and delete. Other item types are
  shown read-only and preserved on edit.
- Three layouts by terminal width (three panes, icon rail, single pane),
  nested folders with fold arrows that are remembered between sessions,
  fuzzy search with qualifiers (`type:`, `folder:`, `is:`, `has:` …) and a
  `Ctrl-p` finder.
- Copying with automatic clipboard clearing (X11, Wayland, OSC 52 over SSH),
  timed reveal, locally computed verification codes, password history and the
  master password re-prompt.
- A form editor with Vim modes, two columns on wide terminals, undo and redo,
  a generator popup, and `$EDITOR` for notes, SSH keys or the whole item as
  JSON, with swap and undo files turned off for common editors.
- Password and passphrase generator, vault health (exposed, reused, weak,
  old and insecure passwords), export in four formats and import with a
  duplicate-aware preview.
- Undo and redo for vault and folder changes, visual mode and marks for bulk
  actions, counts (`3dd`, `12G`) and a jump list.
- Which-key, contextual help, a Vim-like command line with completion and
  history, a settings screen that explains each option, and a keybinding
  editor. Every binding can be changed in `config.toml`, and key hints follow
  your bindings.
- `Ctrl-n` and `Ctrl-p` move through every picker and list as well as the
  arrows: folders, types, themes, formats, quick open, command history,
  the import preview, password history, settings, the keybinding editor
  and help. Picker, quick open and command
  line keys can be rebound in `config.toml`.
- `Ctrl-c` quits from the unlock, login, start-up and error screens, and
  `Ctrl-o` on the unlock screen asks to log out.
- Layouts that keep the essentials at 80×24 and narrower: the session
  state, every health category and the way out of each popup.
- Ten built-in themes with live preview, user themes with `extends` and hot
  reload, and Nerd Font, Unicode or ASCII icons.
- `lazybw doctor`, `keys`, `themes` and `config` subcommands.
- Static binaries for x86_64 and aarch64 (also for Termux on Android),
  `.deb`, `.rpm` and Arch Linux packages, and a container image with `bw`
  on the GitHub Container Registry.
- AUR packages: `lazybw-bin` (the release binary) and `lazybw` (built from
  source).
- A user guide (mdBook) and a logo, also shown in the top bar with Nerd Font
  icons.

### Security

- No Bitwarden cryptography in lazybw and no network access of its own.
- Secrets never in process arguments, zeroized in memory, redacted in logs,
  and dropped on lock. The process is non-dumpable with core dumps disabled.
- Private (0600) files written without following symlinks, temporary files
  in `$XDG_RUNTIME_DIR` that are shredded after use, and no folder names on
  disk.
- The terminal input is released to `$EDITOR` and `bw login` while they run;
  clipboard clearing and the lock signal keep working meanwhile, and a lock
  discards what the editor returns.
- Vim, Neovim, Emacs and micro run without swap, undo or backup files, even
  when their own configuration enables them; Vim modelines are off.
- Links that are not web addresses ask before opening.
- Answers from `bw` that arrive after a lock are dropped.
- Drawing an item, listing cards and building the health report borrow the
  vault data instead of copying passwords, card numbers and notes.

[Unreleased]: https://github.com/SyscallBrain/lazybw/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/SyscallBrain/lazybw/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/SyscallBrain/lazybw/releases/tag/v1.0.0
