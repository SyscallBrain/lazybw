# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/SyscallBrain/lazybw/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/SyscallBrain/lazybw/releases/tag/v1.0.0
