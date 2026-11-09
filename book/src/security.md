# How lazybw keeps secrets safe

lazybw sees every secret you open, so it handles them with care at each step:
in memory, on disk, on the clipboard and in the terminal.

## Bitwarden does the cryptography

lazybw contains no Bitwarden cryptography. It starts the official `bw serve`
and sends it requests; `bw` decrypts and encrypts. lazybw itself never makes
network connections. Everything goes through `bw`, which talks to your
Bitwarden server, and, for the breach check, to Have I Been Pwned.

## A private channel to `bw`

`bw serve` normally listens on a TCP port, which any program on the machine
could connect to. lazybw starts it on one end of a socket pair instead, a
private channel that exists only between the two processes. No port is open
and no socket file exists on disk. You can check with `ss -tlpn` and
`ss -xlpn` while lazybw runs.

When lazybw exits, crashes or is killed, `bw serve` stops too: the kernel
sends it a signal when its parent dies.

## Nothing secret on the command line

The master password, the session key and decrypted values never appear in
process arguments, where other users could see them with `ps`. They travel
through environment variables of the child process or through the private
channel.

## Secrets in memory

- Decrypted values are kept in a wrapper that overwrites the memory when the
  value is dropped, and prints as `***` if it ever ends up in a log.
- What you type into password fields (unlock, re-prompt, export passwords,
  API secret) lives in a buffer that is wiped as you edit and when it is
  dropped.
- Locking drops the decrypted vault, the undo history, the generator
  history, search results and copies of items, and clears the clipboard if it
  still holds a value lazybw copied.
- lazybw marks its process as not dumpable and turns off core dumps, so a
  crash does not write the vault to a core file and other processes of the
  same user cannot read its memory.

## Files on disk

- Files that can hold secrets are created with mode 600 (readable only by
  you), and lazybw starts with a umask of 077.
- Temporary files, such as notes opened in `$EDITOR` or a filtered import,
  live in `$XDG_RUNTIME_DIR/lazybw`, which is in memory on most systems. They
  are overwritten and deleted as soon as they are no longer needed.
- For Vim, Neovim, Emacs and micro, lazybw switches off swap, undo, backup and
  history files, so the editor does not keep its own copy of the text.
- The folder state file stores folder ids, never names.
- The log (`~/.local/state/lazybw/lazybw.log`) never contains requests,
  responses or secret values, even with `LAZYBW_LOG=debug`. Only you can
  read it.

## The terminal

While `$EDITOR` or `bw login` runs, lazybw stops reading the keyboard, so
what you type there, including the master password, goes only to that
program.

## The clipboard

Copied values are cleared after 30 seconds, on lock and on exit, but only if
the clipboard still holds the value lazybw put there. Clipboard managers such
as cliphist or Klipper may keep their own history. Check their settings if
that matters to you.

## Re-prompt and reveal

Items marked "master password re-prompt" in Bitwarden ask for the master
password before any secret in them is shown, copied or changed. Revealed
values hide again after 20 seconds, when you move to another item, and when
the terminal window loses focus.

## Limits

These protections narrow what can leak; they do not make a compromised
machine safe. In particular:

- While the vault is unlocked, decrypted data is in memory: in `bw serve`
  (a Node.js process lazybw does not control), in lazybw's cache, and in the
  text drawn on the screen. Zeroizing removes lazybw's copies when they are
  dropped; it cannot reach copies made by other programs or by the terminal.
- With the clipboard over OSC 52 (over SSH, in a container), lazybw cannot
  read the clipboard back, so it clears it when the timeout ends even if you
  copied something else in the meantime. Clipboard managers and terminals
  may keep their own history.
- Editors other than Vim, Neovim, Emacs and micro get no special options;
  check that yours does not keep backups or swap files of the notes you edit.
- `bw.transport = "unix"` puts the socket in lazybw's private runtime
  directory instead of using a socket pair.
- lazybw protects against other users and programs reading its files; it
  cannot protect against someone with your account or root on the machine.

## Reporting a problem

If you find a security problem, please report it privately through GitHub's
security advisories for the repository instead of opening a public issue.
