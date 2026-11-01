# Keybindings

Every key in lazybw can be changed. The full list of defaults is in
[Default keybindings](reference/keymap.md).

## How bindings are organised

Keys belong to a context: `global`, `sidebar`, `list`, `detail`, `editor`,
`insert`, `generator`, `health` and a few more for popups. lazybw looks in the
most specific context first, so `dd` can mean "move to trash" in the list and
"delete folder" in the filters pane.

Keys are written in Vim notation:

| Notation | Key |
|---|---|
| `a`, `G`, `?` | a printable key |
| `<CR>`, `<Esc>`, `<Tab>`, `<BS>`, `<Del>`, `<Space>` | named keys |
| `<Up>`, `<PageDown>`, `<F5>` | arrows, paging, function keys |
| `<C-p>`, `<A-x>`, `<S-Tab>` | Ctrl, Alt and Shift combinations |
| `gg`, `<leader>fn` | sequences |

## In the configuration file

```toml
[keys]
leader = "<Space>"

[keys.global]
"<leader>tt" = "theme.set tokyonight-storm"

[keys.list]
"dd" = "noop"              # remove a default binding
"<Del>" = "item.trash"     # add another key for an action
"yl" = "item.copy uri"     # an action with an argument
"<leader>w" = ":sync!"     # run a command-line command
```

A binding replaces the default with the same keys in the same context, and
`"noop"` removes one. The action keeps its other keys. With
`keys.defaults = false`, lazybw starts from an empty keymap and uses only
yours.

`[keys.labels]` sets the text which-key shows for a prefix:

```toml
[keys.labels]
"<leader>x" = "+extras"
```

## With the keybinding editor

`Space` `k` opens a table of every action with its keys.

| Keys | Does |
|---|---|
| `Tab` / `Shift-Tab` | switch context |
| `/` | filter by description, action or key |
| `Enter` or `c` | rebind: press the new keys, then `Enter` |
| `a` | add another key to the action |
| `dd` | remove a key |
| `r` / `R` | reset the action / the whole context |
| `Ctrl-s` or `:w` | save |

Changed rows have a dot in front. Saving writes only your differences from
the defaults into `config.toml`.

If the keys you pick are already taken in that context, lazybw tells you
which action uses them and asks before replacing it. To bind `Enter`,
`Backspace` or `Escape`, edit the file, because the editor uses those keys
itself.

## For one session

`:map list gm item.move` adds a binding until you quit, and
`:unmap list gm` removes it. `:map` on its own lists all bindings.

## Checking your bindings

```sh
lazybw keys                  # every binding, grouped by context
lazybw keys --context list   # only one context
lazybw keys --defaults       # the built-in defaults, ignoring your file
lazybw keys --check          # validate; exits with an error code on mistakes
```

`--check` catches unknown actions (and suggests the closest name), actions
used in a context where they make no sense, and keys that would make text
impossible to type.

## Terminals and special keys

Some terminals cannot tell `Ctrl-i` from `Tab`, `Ctrl-m` from `Enter` or
`Ctrl-[` from `Esc`. lazybw turns on the keyboard protocol that separates them
where the terminal supports it: kitty, foot, WezTerm, Ghostty and recent
Alacritty. Inside tmux, add `set -g extended-keys on` to `tmux.conf`.

## Moving in pickers and lists

Every picker (folder, item type, theme, format) and every list in a popup
moves with the arrows or with `Ctrl-n` (next) and `Ctrl-p` (previous), and
so do the settings screen, the keybinding editor and the help. In
the pickers, quick open and the command line, letters are typed into the
filter, so those keys are the way to move. The picker, quick open and
command line keys can be rebound under `[keys.picker]`, `[keys.quick_open]`
and `[keys.cmdline]`.
