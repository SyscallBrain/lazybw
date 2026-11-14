# Default keybindings

Every binding below can be changed; see [Keybindings](../keybindings.md).
`<leader>` is `Space` unless you set `keys.leader`.

## Key notation


Canonical notation is Vim's. Parsing is case-insensitive for names inside `<>` and case-sensitive for
plain characters.

| Form | Examples | Notes |
|---|---|---|
| Printable character | `j`, `G`, `*`, `/`, `?`, `:` | `G` is Shift+g; never write `<S-g>` (normalised to `G`) |
| Named keys | `<CR>` (`<Enter>`, `<Return>`), `<Esc>`, `<Tab>`, `<S-Tab>`, `<BS>`, `<Del>`, `<Space>`, `<Up>`, `<Down>`, `<Left>`, `<Right>`, `<Home>`, `<End>`, `<PageUp>`, `<PageDown>`, `<Insert>`, `<F1>`…`<F12>`, `<lt>` (literal `<`), `<Bar>` (`|`) | |
| Modifiers | `<C-d>`, `<A-x>` (= `<M-x>`), `<C-A-x>`, `<S-Up>`, `<C-S-x>` | `<C-S-…>` needs keyboard enhancement (kitty, foot, WezTerm, Ghostty; under tmux `set -g extended-keys on`) |
| Leader | `<leader>` | expands to `keys.leader` |
| Sequences | `gg`, `<leader>fn`, `<C-w>h`, `zR` | concatenation |

Equivalences applied at parse time: `<C-I>` = `<C-i>`; `<S-a>` = `A`; `<Return>` = `<CR>`; `<M-x>` =
`<A-x>`. Without keyboard enhancement, also `<C-i>` = `<Tab>`, `<C-m>` = `<CR>`, `<C-[>` = `<Esc>`,
`<C-h>` = `<BS>`.

Display (hints, which-key, help, statusline): `Ctrl-d`, `Alt-x`, `Shift-Tab`, `Enter`, `Esc`, `Space`;
plain characters are joined (`gg`, `yp`), named keys are separated by spaces (`Space f n`, `Ctrl-w h`).

## Bindings by context


### `global`

| Keys | Action |
|---|---|
| `q` | `app.quit` |
| `<Esc>` `<C-c>` | `app.cancel` |
| `?` | `app.help` |
| `:` | `cmdline.open` |
| `<C-p>` | `quick_open` |
| `<C-l>` | `app.redraw` |
| `h` `<C-w>h` `<Left>` | `focus.left` |
| `l` `<C-w>l` `<Right>` | `focus.right` |
| `<Tab>` / `<S-Tab>` | `focus.next` / `focus.prev` |
| `gt` / `gT` | `tab.next` / `tab.prev` |
| `<leader>1` `<leader>2` `<leader>3` | `tab.goto 1/2/3` (Vault, Generator, Health) |
| `/` / `n` / `N` | `search.start` / `search.next` / `search.prev` |
| `R` | `vault.sync` |
| `<C-o>` / `<C-i>` | `jump.back` / `jump.forward` |
| `<leader>/` | `quick_open` |
| `<leader>n` | `item.new` |
| `<leader>g` / `<leader>h` | `generator.open` / `health.open` |
| `<leader>e` / `<leader>i` | `vault.export` / `vault.import` |
| `<leader>S` | `vault.sync force` |
| `<leader>l` | `vault.lock` |
| `<leader>t` | `theme.pick` |
| `<leader>b` | `layout.toggle_sidebar` |
| `<leader>d` | `list.toggle_density` |
| `<leader>m` | `app.messages` |
| `<leader>,` | `app.settings` |
| `<leader>k` | `app.keymap` |
| `<leader>q` | `app.quit` |
| `<leader>fn` / `<leader>fr` / `<leader>fd` / `<leader>ff` | `folder.new` / `folder.rename` / `folder.delete` / `folder.jump` |
| `<leader>sn` / `<leader>sm` / `<leader>sc` / `<leader>st` / `<leader>sr` | sort by name / modified / created / type, `sort.reverse` |

Which-key group labels: `<leader>f` "+folders", `<leader>s` "+sort", `g` "+go", `z` "+folds and reveal",
`y` "+copy", `d` "+delete", `<C-w>` "+window".

### Movement (shared by `sidebar`, `list`, `detail`, `visual`, `editor`, `generator`, `health`, `settings`, `keymap_editor`, `help`)

| Keys | Action |
|---|---|
| `j` `<Down>` / `k` `<Up>` | `nav.down` / `nav.up` |
| `gg` / `G` | `nav.top` / `nav.bottom` |
| `<C-d>` / `<C-u>` | `nav.half_down` / `nav.half_up` |
| `<C-f>` `<PageDown>` / `<C-b>` `<PageUp>` | `nav.page_down` / `nav.page_up` |
| `H` / `M` / `L` | screen top / middle / bottom (not in `detail`/`editor`) |
| `zz` / `zt` / `zb` | `view.center` / `view.top` / `view.bottom` |

### `sidebar`

| Keys | Action |
|---|---|
| `<CR>` | `filter.apply` |
| `l` `<Right>` | `fold.expand_or_apply` (open a closed folder, else show its items) |
| `h` `<Left>` | `fold.collapse_or_parent` (close an open folder, else go to its parent) |
| `za` / `zo` / `zc` / `zR` / `zM` | `fold.toggle` / `fold.open` / `fold.close` / `fold.open_all` / `fold.close_all` |
| `a` / `r` / `dd` | `folder.new` / `folder.rename` / `folder.delete` |
| `u` / `<C-r>` | `undo` / `redo` |

### `list`

| Keys | Action |
|---|---|
| `<CR>` `l` | `item.open` |
| `yy` | `item.copy primary` |
| `yu` / `yp` / `yt` / `yU` / `yn` | `item.copy username/password/totp/uri/notes` |
| `a` | `item.new` (type picker) |
| `e` `i` | `item.edit` |
| `gE` | `item.edit_json` |
| `dd` | `item.trash` |
| `D` | `item.delete` |
| `r` | `item.restore` (Trash filter) |
| `X` | `trash.empty` (Trash filter) |
| `u` / `<C-r>` | `undo` / `redo` |
| `*` | `item.favorite` |
| `m` | `item.move` |
| `Y` / `p` | `item.yank` / `item.paste_clone` |
| `gx` | `item.open_uri` |
| `gh` | `item.history` |
| `gp` | `item.generate_password` |
| `gf` | `item.goto_folder` |
| `K` | `item.info` |
| `za` / `zR` / `zM` | `item.reveal_toggle` / `item.reveal_all` / `item.hide_all` |
| `V` | `visual.start` |
| `x` | `mark.toggle` |

### `detail`

| Keys | Action |
|---|---|
| `j` / `k` | next / previous field (movement) |
| `yy` | `item.copy_field` |
| `yu` / `yp` / `yt` / `yU` / `yn` | as in `list` |
| `za` / `zR` / `zM` | `item.reveal_toggle` / `item.reveal_all` / `item.hide_all` |
| `i` / `cc` | `item.edit_field` / `item.change_field` |
| `e` | `item.edit` |
| `gx` | `item.open_uri` (the URI under the cursor) |
| `gh` / `gp` / `gf` / `K` / `*` / `m` | as in `list` |
| `u` / `<C-r>` | `undo` / `redo` |

### `visual`

| Keys | Action |
|---|---|
| `j` `k` `gg` `G` … | extend the range |
| `x` | `mark.toggle` |
| `d` | `item.trash` (selection) |
| `D` / `r` | `item.delete` / `item.restore` (Trash filter) |
| `m` | `item.move` (selection) |
| `*` | `item.favorite` (sets all to favorite unless all already are, then removes) |
| `<Esc>` `V` | `visual.exit` |

### `editor` (NORMAL)

| Keys | Action |
|---|---|
| `j` `<Down>` `<Tab>` / `k` `<Up>` `<S-Tab>` | `editor.next_field` / `editor.prev_field` |
| `gg` / `G` | `editor.first_field` / `editor.last_field` |
| `i` / `a` / `I` / `A` | insert / append / insert at start / append at end |
| `cc` `S` | `editor.change_field` |
| `o` / `O` | `editor.row_add_below` / `editor.row_add_above` |
| `dd` | `editor.row_delete` |
| `<C-a>` / `<C-x>` | `editor.cycle_next` / `editor.cycle_prev` |
| `<CR>` | `editor.activate` |
| `za` | `item.reveal_toggle` |
| `<C-g>` | `editor.generate` |
| `<C-e>` | `editor.external` |
| `gE` | `item.edit_json` |
| `u` / `<C-r>` | `editor.undo` / `editor.redo` |
| `<C-s>` | `editor.save` |
| `ZZ` / `ZQ` | `editor.save_close` / `editor.discard` |
| `q` | `editor.close` |
| `h` / `l` | `editor.column_left` / `editor.column_right` |

### `insert`

| Keys | Action |
|---|---|
| `<Esc>` `<C-[>` `<C-c>` | `editor.normal` |
| `<Tab>` `<CR>` / `<S-Tab>` | `editor.next_field` / `editor.prev_field` (stay in INSERT) |
| `<C-a>` / `<C-x>` | `editor.cycle_next` / `editor.cycle_prev` (on option fields) |
| `<C-g>` | `editor.generate` |
| `<C-e>` | `editor.external` |
| `<C-s>` | `editor.save` |

Fixed line-editing keys in INSERT (not remappable): `<BS>`, `<Del>`, `<Left>`, `<Right>`, `<Home>`,
`<End>`, `<C-w>` (delete word), `<C-u>` (delete to start), `<C-h>` (= `<BS>`), bracketed paste.

### `notes`

edtui provides Vim NORMAL/INSERT/VISUAL inside the notes field. lazybw binds only: `<Tab>`/`<S-Tab>` in
NORMAL to leave the field, `<C-e>` external editor, `<C-s>` save, and `:` to open the lazybw command line
(so `:w`, `:wq`, `:q!` keep working).

### `generator`

| Keys | Action |
|---|---|
| `r` | `generator.regenerate` |
| `yy` | `generator.copy` |
| `<Tab>` | `generator.toggle_mode` |
| `l` `<C-a>` `+` / `h` `<C-x>` `-` | `generator.increase` / `generator.decrease` |
| `x` | `generator.toggle_option` |
| `<CR>` | `generator.use` (popup) / `generator.toggle_option` (tab, on a checkbox) |

### `health`

| Keys | Action |
|---|---|
| `<Tab>` / `<S-Tab>` | `health.next_category` / `health.prev_category` |
| `<CR>` | `health.open_item` |
| `e` / `gp` | `item.edit` / `item.generate_password` |
| `r` | `health.rerun` |

### `quick_open`

| Keys | Action |
|---|---|
| `<C-n>` `<C-j>` `<Down>` / `<C-p>` `<C-k>` `<Up>` | `quick_open.next` / `quick_open.prev` |
| `<CR>` | `quick_open.accept` |
| `<C-y>` / `<C-u>` / `<C-t>` | copy password / username / code |
| `<C-e>` | `quick_open.edit` |
| `<Esc>` `<C-c>` | `app.cancel` |

### `cmdline`

| Keys | Action |
|---|---|
| `<CR>` | `cmdline.execute` |
| `<Tab>` / `<S-Tab>` | complete next / previous |
| `<Up>` `<C-p>` / `<Down>` `<C-n>` | history previous / next |
| `<Esc>` `<C-c>` | `app.cancel` |

### `picker`, `confirm`, `unlock`, `settings`, `keymap_editor`, `help`

| Context | Keys → action |
|---|---|
| picker | type to filter; `<C-n>` `<Down>` / `<C-p>` `<Up>` next/prev; `<CR>` accept; `<Esc>` cancel. These come from the keymap, so they can be rebound. Lists that are not keymap contexts (export format, import preview, password history, login choices) also take `<C-n>` / `<C-p>` beside the arrows and `j` / `k`. Small pickers (item type) also accept single-letter accelerators shown in the list (`l` login, `n` note, `c` card, `i` identity, `s` SSH key). |
| confirm | `y` / `<CR>` (only when the default is yes) → yes; `n` / `<Esc>` → no. Destructive dialogs require typing a word instead. |
| unlock | `<CR>` submit; `<C-o>` log out; `<C-c>` quit. The master password re-prompt shares this context but `<C-c>` cancels it, like `<Esc>`. The login, start-up and error screens are not keymap contexts; `<C-c>` quits from them too. |
| settings | movement, also `<C-n>` / `<C-p>`; `<CR>` activate; `l` `<C-a>` / `h` `<C-x>` next/previous value; `r` reset to default; `q` `<Esc>` close. |
| keymap_editor | movement, also `<C-n>` / `<C-p>`; `/` filter; `<CR>` `c` rebind; `a` add key; `dd` remove key; `r` reset action; `R` reset all; `<Tab>`/`<S-Tab>` context; `<C-s>` save; `q` close. |
| help | movement, also `<C-n>` / `<C-p>`; `/` search; `q` `<Esc>` close. |

## Command line


`:` opens the command line at the bottom (cmdline row). Completion: commands first, then arguments
(themes, folders, item types, settings and their values, export formats, import formats, filesystem paths
with `~` expansion). History is kept between sessions. Unknown commands print
`E492: Not an editor command: <text>` (Vim's message).

| Command | Action |
|---|---|
| `:q[uit]` | `app.quit` (in editor: `editor.close`) |
| `:q!` | `editor.discard` in editor, otherwise `app.quit_force` |
| `:qa[ll]` / `:qa!` | `app.quit` / `app.quit_force` |
| `:w[rite]` | `editor.save` (keymap editor: `keymap.save`) |
| `:wq` `:x` | `editor.save_close` |
| `:e!` | `editor.reload` |
| `:{n}` | go to item number n in the list |
| `:new [login\|note\|card\|identity\|ssh]` | `item.new` |
| `:edit` / `:edit json` | `item.edit` / `item.edit_json` |
| `:delete` / `:delete!` | `item.trash` / `item.delete` |
| `:restore` | `item.restore` |
| `:fav` | `item.favorite` |
| `:move [folder]` | `item.move` |
| `:copy <field>` | `item.copy <field>` |
| `:open [n]` | `item.open_uri` |
| `:history` | `item.history` |
| `:gen [length]` / `:gen phrase [words]` | generate with these options and copy |
| `:folder new\|rename\|delete [name]` | `folder.*` |
| `:sort name\|modified\|created\|type[!]` | `sort.set` (`!` reverses) |
| `:theme [name]` | `theme.set` / `theme.pick` |
| `:set` | `app.settings` |
| `:set opt=value` / `:set opt?` / `:set opt!` / `:set opt&` | set / show / toggle boolean / reset |
| `:map [ctx] [keys] [action…]` / `:unmap ctx keys` | session mappings (see [Keybindings](../keybindings.md)) |
| `:keymap` | `app.keymap` |
| `:sync` / `:sync!` | `vault.sync` / `vault.sync force` |
| `:lock` / `:logout` | `vault.lock` / `vault.logout` |
| `:export [json\|encrypted\|account\|csv] [path]` | `vault.export` (opens the wizard pre-filled) |
| `:import [format] [path]` | `vault.import` (wizard pre-filled) |
| `:health` | `health.open` |
| `:trash` | sidebar filter Trash |
| `:empty-trash` | `trash.empty` |
| `:messages` | `app.messages` |
| `:help [topic]` | `app.help` |
| `:doctor` | `app.doctor` |

`:set` option names are the config keys without the section where unambiguous (`:set theme=nord`,
`:set clipboard_timeout=20s`, `:set animations!`); fully qualified names always work
(`:set ui.animations=false`).

## Search language


Used by `/` (inside the current sidebar filter) and by quick open (whole vault, trash excluded unless
`is:trash`).

- Tokens are separated by spaces; `"quoted phrases"` keep spaces; `-` negates a token or qualifier.
- Free text is fuzzy-matched with `nucleo-matcher` (smart case, Unicode normalisation) against:
  name (weight 1.0), username (0.7), URI hosts (0.6), custom field **names** (0.4), notes (0.3, only when
  `vault.search_notes = true`). Item score = best weighted score; ties: favorites first, then items opened
  in this session (most recent first), then name.
- Matched characters of the name are highlighted with the `match` token.

| Qualifier | Values |
|---|---|
| `type:` | `login`/`l`, `note`/`n`, `card`/`c`, `identity`/`i`, `ssh`/`s` |
| `folder:` | case-insensitive prefix of the folder path (`folder:work/proj`), `none` |
| `is:` | `fav`, `trash`, `reprompt`, `weak`, `reused`, `exposed`, `old`, `insecure` (the last five use the latest health run; without one they are ignored and the statusline says "Run Health first") |
| `has:` | `totp`, `passkey`, `fields`, `notes`, `uri`, `history`, `attachments` |
| `uri:` | substring of any URI |
| `user:` | substring of the username |
| `name:` | substring of the name |

Examples: `git -type:note`, `has:totp folder:work`, `is:reused uri:google`, `"home router"`.
