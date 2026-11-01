# Folders

Bitwarden folders can be nested by naming them with a slash, such as
`Finanças/Bancos`. lazybw shows them as a tree in the filters pane.

## Opening and closing folders

A folder that has subfolders has an arrow in front of it: `▾` when it is open
and `▸` when it is closed. In the filters pane:

| Keys | Does |
|---|---|
| `l` or `→` | open a closed folder; on an open folder or one without subfolders, show its items |
| `h` or `←` | close an open folder; on a subfolder, go up to its parent |
| `Enter` | show the folder's items |
| `za` | open or close the folder under the cursor |
| `zR` / `zM` | open / close every folder |

Clicking the arrow with the mouse opens or closes the folder as well.

lazybw remembers which folders you closed, also after you quit. It stores
them by their Bitwarden id in `~/.local/state/lazybw/folds.json`. Folder names
are encrypted in your vault, so lazybw never writes them to that file. Since
the id stays the same when you rename a folder, a closed folder stays closed
after a rename too.

## Going to a folder

`Space` `f` `f` opens a list of all folders. Type to filter it and press
`Enter` to show that folder's items. "No folder" at the top shows the items
that are not in any folder.

`gf` on an item jumps to its folder.

## Moving items

`m` opens a folder list for the item under the cursor. Pick a folder, or "No
folder" to take the item out of its current one. To move several items at
once, select them first (see [bulk actions](trash-and-undo.md#working-on-several-items)).

The same list has "+ Create folder…" at the top, which asks for a name and
creates the folder.

## Creating, renaming and deleting folders

| Keys | In the filters pane | Anywhere |
|---|---|---|
| New folder | `a` | `Space` `f` `n` |
| Rename | `r` | `Space` `f` `r` |
| Delete | `dd` | `Space` `f` `d` |

Renaming a folder that has subfolders offers to rename the subfolders too,
so `Work/Projects` follows when `Work` becomes `Job`. Deleting a folder moves
its items to No folder; the dialog asks whether to delete the subfolders as
well. All three can be undone with `u`.
