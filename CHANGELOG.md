# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Cancelling a key sequence with `Esc` (for example `Space` then `Esc`)
  no longer leaves the keys shown in the statusline.

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
