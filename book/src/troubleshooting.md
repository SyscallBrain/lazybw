# Troubleshooting

Start with:

```sh
lazybw doctor
```

It shows the lazybw version and what lazybw sees: the `bw` path and version,
the login state, the clipboard backend, the terminal and the file paths. The
log is in `~/.local/state/lazybw/lazybw.log`, and `Space` `m` shows the
messages of the current session.

The log records when lazybw starts and exits, which `bw` it found and what
went wrong. For more detail, such as the error output of `bw`, start lazybw
with `LAZYBW_LOG=debug lazybw` and repeat what failed.

## lazybw says `bw` was not found or is too old

Install the Bitwarden CLI as described in [Installation](installation.md)
and check `bw --version`. If `bw` is not on your `PATH`, set `bw.path` in
the configuration or the `LAZYBW_BW` environment variable.

## Colours look grey or wrong

lazybw uses 24-bit colour when the terminal says it supports it. Check that
`echo $COLORTERM` prints `truecolor`. Over SSH and inside tmux this variable is
often lost; set it in your shell profile, or in tmux with
`set -as terminal-features ",*:RGB"`. The `terminal` theme uses your
terminal's own palette and works everywhere.

## Icons show as boxes or question marks

Your terminal font has no Nerd Font icons. Install a Nerd Font and select it
in the terminal, or set `ui.icons = "unicode"`.

## Some key combinations do nothing

Many terminals send the same code for `Ctrl-i` and `Tab`, `Ctrl-m` and
`Enter`, or `Ctrl-[` and `Esc`. kitty, foot, WezTerm, Ghostty and recent
Alacritty can tell them apart, and lazybw switches that on automatically.
In tmux, add `set -g extended-keys on`. `lazybw keys --check` warns about
bindings your terminal cannot distinguish.

## Copying does nothing

`lazybw doctor` shows the clipboard backend. Under Wayland, install
`wl-clipboard`; under X11, `xclip` or `xsel` work as fallbacks. Over SSH,
lazybw uses OSC 52, which your local terminal must allow (in tmux:
`set -g set-clipboard on`).

## The screen shows leftover characters

Press `Ctrl-l` to redraw everything. If it keeps happening, please open an
issue with your terminal, its version and what you were doing.

## Sync errors or "Vault is locked"

The session may have expired on the server. Lock with `Space` `l` and unlock
again. If that fails, log out with `:logout` and log in once more.

## Reporting a bug

Open an issue on GitHub with the output of `lazybw doctor`, the last lines of
the log and the steps that lead to the problem. Neither contains passwords or
vault data, though the `doctor` output includes your email address, which you
may want to remove.
