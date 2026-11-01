# lazybw — Keymap, Commands and Customization

lazybw speaks Vim. Every user-invokable behaviour is an **action** in one registry. Keys, `:`
commands, the which-key popup, the help overlay, the keymap editor and `lazybw keys` all read that
registry, so they can never disagree.

---

## 1. Model

### 1.1 Contexts

| Context | Active when | Text entry? |
|---|---|---|
| `global` | always, lowest priority (not inside modal overlays) | no |
| `sidebar` | Vault tab, sidebar focused | no |
| `list` | Vault tab, item list focused | no |
| `detail` | Vault tab, detail pane focused | no |
| `visual` | list focused with an active selection (`V` or marks) — checked before `list` | no |
| `editor` | item editor, NORMAL mode | no |
| `insert` | item editor, INSERT mode in a single-line field | **yes** |
| `notes` | item editor, notes field (edtui handles Vim keys; only listed bindings are lazybw's) | yes |
| `generator` | Generator tab or generator popup | no |
| `health` | Health tab | no |
| `quick_open` | `Ctrl-p` finder | **yes** |
| `cmdline` | `:` line | **yes** |
| `picker` | folder/theme/format/type pickers | **yes** (filter) |
| `confirm` | confirmation dialogs | no |
| `unlock` | Unlock screen | **yes** |
| `login` | Login screens | mixed (lists no, forms yes) |
| `settings` | Settings overlay | no |
| `keymap_editor` | Keymap editor | no |
| `help` | Help overlay, `:messages`, info popups | no |

### 1.2 Resolution order

1. The topmost overlay's context(s). Overlays are **modal**: if nothing matches there, the key is
   dropped (except `app.cancel` keys, which every overlay context binds).
2. Without overlays: the focused screen's contexts from most to least specific (e.g. `visual` → `list`),
   then `global`.
3. In text-entry contexts, a printable key without modifiers that matches nothing is inserted as text.

### 1.3 Sequences, timeout and which-key

- Bindings are key **sequences** stored in a trie per context (`gg`, `<leader>fn`, `<C-w>h`).
- While the typed keys are a strict prefix of a binding, lazybw waits. If they are also a complete binding,
  it runs after `keys.timeoutlen` (1000 ms) unless another key extends the sequence — Vim's behaviour.
- A key that leaves no possible match discards the pending sequence silently.
- The pending sequence and the count are shown in the statusline (`showcmd` slot).
- After `keys.which_key_delay` (300 ms) with a pending prefix that has children, the which-key popup lists
  the continuations with their descriptions; groups show as `+label`.

### 1.4 Counts

In non-text contexts, `1`–`9` start a count and `0`–`9` continue it (unless the digit is bound in that
context). Actions declare whether they accept a count: navigation (`5j`), `item.trash` (`3dd` trashes 3
items from the cursor), `nav.top`/`nav.bottom` (`12G` goes to item 12). Others ignore it.

### 1.5 Leader

`<leader>` in any binding expands to `keys.leader` (default `<Space>`). Changing the leader re-expands
all bindings, default and user-defined.

## 2. Key notation
<!-- ANCHOR: notation -->

Canonical notation is Vim's. Parsing is case-insensitive for names inside `<>` and case-sensitive for
plain characters.

| Form | Examples | Notes |
|---|---|---|
| Printable character | `j`, `G`, `*`, `/`, `?`, `:` | `G` is Shift+g; never write `<S-g>` (normalised to `G`) |
| Named keys | `<CR>` (`<Enter>`, `<Return>`), `<Esc>`, `<Tab>`, `<S-Tab>`, `<BS>`, `<Del>`, `<Space>`, `<Up>`, `<Down>`, `<Left>`, `<Right>`, `<Home>`, `<End>`, `<PageUp>`, `<PageDown>`, `<Insert>`, `<F1>`…`<F12>`, `<lt>` (literal `<`), `<Bar>` (`|`) | |
| Modifiers | `<C-d>`, `<A-x>` (= `<M-x>`), `<C-A-x>`, `<S-Up>`, `<C-S-x>` | `<C-S-…>` needs keyboard enhancement (§7) |
| Leader | `<leader>` | expands to `keys.leader` |
| Sequences | `gg`, `<leader>fn`, `<C-w>h`, `zR` | concatenation |

Equivalences applied at parse time: `<C-I>` = `<C-i>`; `<S-a>` = `A`; `<Return>` = `<CR>`; `<M-x>` =
`<A-x>`. Without keyboard enhancement, also `<C-i>` = `<Tab>`, `<C-m>` = `<CR>`, `<C-[>` = `<Esc>`,
`<C-h>` = `<BS>`.

Display (hints, which-key, help, statusline): `Ctrl-d`, `Alt-x`, `Shift-Tab`, `Enter`, `Esc`, `Space`;
plain characters are joined (`gg`, `yp`), named keys are separated by spaces (`Space f n`, `Ctrl-w h`).

<!-- ANCHOR_END: notation -->
## 3. Action registry

Arguments in `{}`; `count` = accepts a count. Every action has a one-line description (used by which-key
and help) — the descriptions below are the exact UI strings.

### Application and navigation between areas

| Action | Description | Contexts |
|---|---|---|
| `app.quit` | Quit lazybw | global |
| `app.quit_force` | Quit and discard unsaved changes | global |
| `app.cancel` | Cancel | all |
| `app.help` | Show help for this screen | global |
| `app.redraw` | Redraw the whole screen | global |
| `app.messages` | Show message history | global |
| `app.settings` | Open settings | global |
| `app.keymap` | Edit keybindings | global |
| `app.doctor` | Show diagnostics | global |
| `cmdline.open` | Open the command line | global |
| `quick_open` | Find any item | global |
| `focus.left` / `focus.right` | Focus the pane on the left / right | global |
| `focus.next` / `focus.prev` | Focus the next / previous pane | global |
| `layout.toggle_sidebar` | Show or hide the sidebar | global |
| `list.toggle_density` | Switch between compact and comfortable rows | global |
| `tab.next` / `tab.prev` | Next / previous tab | global |
| `tab.goto {1-3}` | Go to tab | global |
| `jump.back` / `jump.forward` | Jump back / forward | global |
| `theme.pick` | Choose a theme | global |
| `theme.set {name}` | Use theme | global |

### Movement (any list-like context)

| Action | Description |
|---|---|
| `nav.down {count}` / `nav.up {count}` | Move down / up |
| `nav.top {count}` / `nav.bottom {count}` | Go to the first / last row (or row N) |
| `nav.half_down` / `nav.half_up` | Half a page down / up |
| `nav.page_down` / `nav.page_up` | A page down / up |
| `nav.screen_top` / `nav.screen_middle` / `nav.screen_bottom` | Top / middle / bottom of the screen |
| `view.center` / `view.top` / `view.bottom` | Scroll so the cursor is centred / at the top / at the bottom |
| `search.start` | Search in this list |
| `search.next` / `search.prev` | Next / previous match |
| `search.clear` | Clear the search |

### Vault

| Action | Description | Contexts |
|---|---|---|
| `vault.sync {force?}` | Sync with the server | global |
| `vault.lock` | Lock the vault | global |
| `vault.logout` | Log out | global |
| `vault.export` | Export the vault | global |
| `vault.import` | Import into the vault | global |
| `generator.open` | Open the generator | global |
| `health.open` | Open vault health | global |
| `sort.set {name\|modified\|created\|type}` | Sort by … | global |
| `sort.reverse` | Reverse the sort order | global |
| `filter.apply` | Show these items | sidebar |
| `fold.expand_or_apply` | Open folder, or show its items | sidebar |
| `fold.collapse_or_parent` | Close folder, or go to its parent | sidebar |
| `fold.toggle` / `fold.open` / `fold.close` | Toggle / open / close folder | sidebar |
| `fold.open_all` / `fold.close_all` | Open / close all folders | sidebar |
| `folder.new` | New folder | sidebar, global |
| `folder.rename` | Rename folder | sidebar, global |
| `folder.delete` | Delete folder | sidebar, global |
| `folder.jump` | Go to folder | global |
| `trash.empty` | Empty the trash | list (Trash filter) |

### Items

| Action | Description | Contexts |
|---|---|---|
| `item.open` | Open item | list |
| `item.copy {primary\|username\|password\|totp\|uri\|notes\|number\|code\|public_key\|private_key\|field <name>}` | Copy … | list, detail, visual (no) |
| `item.copy_field` | Copy this field | detail |
| `item.reveal_toggle` | Show or hide this field (from the list: the item's first secret) | list, detail, editor |
| `item.reveal_all` / `item.hide_all` | Show / hide all fields | list, detail |
| `item.new {login\|note\|card\|identity\|ssh}` | New item | list, global |
| `item.edit` | Edit item | list, detail |
| `item.edit_field` | Edit this field | detail |
| `item.change_field` | Replace this field | detail |
| `item.edit_json` | Edit as JSON in $EDITOR | list, detail |
| `item.trash {count}` | Move to trash | list, visual |
| `item.delete` | Delete permanently | list (Trash), visual |
| `item.restore` | Restore from trash | list (Trash), visual |
| `item.favorite` | Add to or remove from favorites | list, detail, visual |
| `item.move {folder?}` | Move to folder | list, detail, visual |
| `item.yank` | Copy item for cloning | list |
| `item.paste_clone` | Paste a clone here | list |
| `item.open_uri {n?}` | Open website | list, detail |
| `item.history` | Password history | list, detail |
| `item.info` | Item details | list, detail |
| `item.generate_password` | Generate and save a new password | list, detail, health |
| `item.goto_folder` | Go to this item's folder | list, detail |
| `undo` / `redo` | Undo / redo | list, detail, visual, sidebar |
| `mark.toggle` | Mark or unmark | list, visual |
| `visual.start` | Select a range | list |
| `visual.exit` | Clear the selection | visual |

### Editor

| Action | Description | Contexts |
|---|---|---|
| `editor.save` | Save | editor, insert |
| `editor.save_close` | Save and close | editor |
| `editor.close` | Close | editor |
| `editor.column_left` / `editor.column_right` | Jump to the same row of the left / right column (two-column layout) | editor |
| `editor.discard` | Close without saving | editor |
| `editor.reload` | Reload from the server | editor |
| `editor.next_field` / `editor.prev_field` | Next / previous field | editor, insert |
| `editor.first_field` / `editor.last_field` | First / last field | editor |
| `editor.insert` / `editor.append` | Insert / append | editor |
| `editor.insert_start` / `editor.append_end` | Insert at start / append at end | editor |
| `editor.change_field` | Replace the value | editor |
| `editor.normal` | Back to NORMAL | insert |
| `editor.row_add_below` / `editor.row_add_above` | New row below / above | editor |
| `editor.row_delete` | Delete row or clear field | editor |
| `editor.cycle_next` / `editor.cycle_prev` | Next / previous option | editor, insert |
| `editor.activate` | Open picker or toggle | editor |
| `editor.generate` | Generate password | editor, insert |
| `editor.external` | Open in $EDITOR | editor, insert, notes |
| `editor.undo` / `editor.redo` | Undo / redo change | editor |

In-field Vim motions and operators (`h l w b e 0 ^ $ x dw diw cw ciw D C S p P`, registers) are
implemented by `input/line.rs` and are **not remappable in v1** (documented in the help screen).

### Tools and overlays

| Action | Description | Context |
|---|---|---|
| `generator.regenerate` | Generate another | generator |
| `generator.copy` | Copy | generator |
| `generator.toggle_mode` | Password / passphrase | generator |
| `generator.increase` / `generator.decrease` | Increase / decrease value | generator |
| `generator.toggle_option` | Turn option on or off | generator |
| `generator.use` | Use this password | generator (popup) |
| `health.rerun` | Run the check again | health |
| `health.next_category` / `health.prev_category` | Next / previous category | health |
| `health.open_item` | Open item | health |
| `quick_open.next` / `quick_open.prev` | Next / previous result | quick_open |
| `quick_open.accept` | Open | quick_open |
| `quick_open.copy_password` / `quick_open.copy_username` / `quick_open.copy_totp` | Copy password / username / code | quick_open |
| `quick_open.edit` | Edit | quick_open |
| `cmdline.execute` | Run | cmdline |
| `cmdline.complete_next` / `cmdline.complete_prev` | Next / previous completion | cmdline |
| `cmdline.history_prev` / `cmdline.history_next` | Previous / next command | cmdline |
| `picker.next` / `picker.prev` / `picker.accept` | Next / previous / choose | picker |
| `confirm.yes` / `confirm.no` | Confirm / cancel | confirm |
| `unlock.submit` | Unlock | unlock |
| `unlock.logout` | Log out | unlock |
| `settings.activate` / `settings.next_value` / `settings.prev_value` / `settings.reset` | Change / next / previous / reset setting | settings |
| `keymap.rebind` / `keymap.add` / `keymap.remove` / `keymap.reset` / `keymap.reset_all` / `keymap.save` / `keymap.next_context` / `keymap.prev_context` | Rebind / add key / remove key / reset / reset all / save / next / previous context | keymap_editor |
| `noop` | Do nothing (used to unbind) | all |

## 4. Default bindings
<!-- ANCHOR: bindings -->

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

<!-- ANCHOR_END: bindings -->

## 5. Command line
<!-- ANCHOR: commands -->

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
| `:map [ctx] [keys] [action…]` / `:unmap ctx keys` | session mappings (§6.6) |
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

<!-- ANCHOR_END: commands -->
## 6. Customization

### 6.1 Config format

Bindings live in `~/.config/lazybw/config.toml`:

```toml
[keys]
leader = "<Space>"
timeoutlen = "1000ms"
which_key_delay = "300ms"
defaults = true            # false = start from an empty keymap (only your bindings)

[keys.global]
"<C-p>" = "quick_open"
"<leader>tt" = "theme.set tokyonight-storm"   # action with arguments

[keys.list]
"dd" = "noop"              # remove a default binding
"<Del>" = "item.trash"     # add a key; the action keeps its other keys
"yl" = "item.copy uri"

[keys.labels]
"<leader>x" = "+extras"    # which-key label for a prefix
```

### 6.2 Action strings

`"<action-id> [args…]"` — the same syntax as the command line without `:` (arguments separated by
spaces, quotes for spaces inside an argument). A value starting with `:` runs a command line literally
(`"<leader>w" = ":sync!"`).

### 6.3 Merge rules

1. Start from the defaults (unless `keys.defaults = false`).
2. For each user entry in `[keys.<context>]`: same keys in the same context → replaced; `noop` → removed.
3. Actions may have any number of key sequences; a key sequence maps to one action per context.
4. `<leader>` is expanded after merging.

### 6.4 Validation and conflicts

Run on startup, on hot reload, in the keymap editor and in `lazybw keys --check`.

| Check | Severity | Message example |
|---|---|---|
| Unknown action | error | `keys.list."yl": unknown action "item.cpy" (did you mean "item.copy"?)` |
| Action not available in context | error | `keys.sidebar."dd": "item.trash" is not available in sidebar` |
| Invalid notation | error | `keys.global."<C-": unterminated "<" at column 1` |
| Bad arguments | error | `keys.list."yz": item.copy expects one of primary, username, …` |
| Printable key without modifier in a text-entry context | error | `keys.cmdline."j": would make "j" impossible to type` |
| Modal context without a way out | error | `keys.picker: nothing is bound to app.cancel` |
| A sequence is both a binding and a prefix of another | warning | `keys.list: "g" waits 1000 ms because "gg", "gt"… start with it` |
| User binding shadows a global binding | info | `keys.list."R" shadows global "vault.sync"` |
| Distinct bindings that the terminal cannot tell apart | warning | `"<C-i>" and "<Tab>" are the same key without keyboard enhancement` |

Errors at startup: the config error screen (file, line, message). Errors on hot reload: a toast, previous
keymap kept. Warnings/info: `:messages` and `lazybw keys --check`.

Required unit tests (`config/keymap.rs`): every notation form and equivalence; invalid notations; trie
insert/lookup/prefix; timeout resolution of `g` vs `gg`; counts (`10j`, `0` not a count at start, `3dd`);
leader change re-expansion; replace/add/`noop`/`defaults = false`; each validation row above; display
formatting.

### 6.5 Keymap editor (`<leader>k`, `:keymap`, Settings → Keybindings)

Layout in design.md §4.16. Behaviour:

- Table of actions for the selected context (`<Tab>` cycles contexts; the first entry is "All").
  Columns: action description, action id (muted), keys. User-changed rows show a `●` in the accent colour.
- `/` filters by description, id or key.
- `<CR>`/`c` **rebind**: opens the capture popup "Press the new keys for “Move to trash” (List)".
  Every key press is recorded literally (no actions run); `<BS>` removes the last key; `<CR>` confirms;
  `<Esc>` cancels. Since `<CR>`, `<BS>` and `<Esc>` are captured as controls, a second line says
  "To bind Enter, Backspace or Escape, type `<CR>`, `<BS>` or `<Esc>` in the config file."
  After confirming, validation runs: if the sequence is taken in that context, ask "`dd` is used by
  “Move to trash”. Replace it? y/n".
- `a` adds another key to the action (same capture flow). `dd` removes the key under the cursor (if the
  action has several keys, a small picker asks which). `r` resets the action to its defaults. `R` resets
  the whole context (confirmation).
- Changes apply immediately in the session; the title shows `[+]` until saved. `<C-s>`/`:w` writes the
  **difference from the defaults** into `[keys.<context>]` using `toml_edit`, preserving comments and
  formatting; removed defaults are written as `noop`. `q` with unsaved changes asks
  "Save keybinding changes? y/n/c".

### 6.6 `:map` and `:unmap`

- `:map` lists effective bindings (all contexts) in a scrollable popup.
- `:map list gm item.move` binds for this session only. `:unmap list gm` removes.
- Session mappings are not persisted; the keymap editor shows them with a `session` badge and can save
  them.

### 6.7 `lazybw keys`

```
lazybw keys                     # effective keymap, grouped by context
lazybw keys --context list      # one context
lazybw keys --defaults          # built-in defaults only
lazybw keys --check             # validate config; exit 1 on errors, 0 otherwise (warnings printed)
lazybw keys --format toml       # print as a [keys.*] TOML snippet to paste into config.toml
```

Columns: context, keys (canonical notation), action, description, source (`default` / `user`).

## 7. Terminal caveats

- Enable crossterm keyboard enhancement (`DISAMBIGUATE_ESCAPE_CODES | REPORT_ALTERNATE_KEYS`) when
  `supports_keyboard_enhancement()` is true (kitty, foot, WezTerm, Ghostty, recent Alacritty). Pop the flags
  on exit. Without it, `<C-i>`/`<Tab>`, `<C-m>`/`<CR>`, `<C-[>`/`<Esc>` and `<C-h>`/`<BS>` are identical,
  and `<C-S-x>` is unavailable.
- Under tmux, keyboard enhancement needs `set -g extended-keys on`; `lazybw doctor` prints this hint when
  `$TMUX` is set.
- `<S-Tab>` arrives as `BackTab`; normalise.
- Alt bindings rely on the terminal sending ESC-prefixed sequences (default on Linux terminals).

## 8. Undo and redo semantics

Operations are recorded as reversible entries with a description (used in toasts: "Undone: moved 3 items
to Trash"). Bulk actions are one entry. The stack holds 100 entries and is cleared on lock.

| Operation | Undo | Redo |
|---|---|---|
| Create item | move it to trash | restore it |
| Edit item | PUT the previous full JSON (rebased on the current server copy) | PUT the new JSON |
| Move to trash | restore | move to trash |
| Restore | move to trash | restore |
| Favorite | toggle back | toggle |
| Move to folder | move back to each item's previous folder | move again |
| Clone | move the clone to trash | restore it |
| Folder create | delete it (only if still empty; otherwise the entry is skipped with a message) | create |
| Folder rename (+ subfolders) | rename back | rename |
| Folder delete | recreate the folder(s) and move the former items back | delete again |
| Permanent delete, empty trash, import, export | not undoable — these always ask for confirmation | — |

Undoing a password change restores the old password, but the server keeps the intermediate password in
the item's history; the toast says "Password restored. The history keeps the newer password."

## 9. Search query language
<!-- ANCHOR: search -->

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
<!-- ANCHOR_END: search -->
