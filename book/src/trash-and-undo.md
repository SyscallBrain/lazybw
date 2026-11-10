# Trash, undo and bulk actions

## The trash

`dd` moves the item under the cursor to the trash. Trashed items stay in
Bitwarden's trash for 30 days, where you can bring them back.

Select Trash in the filters pane (or type `:trash`) to see them. There:

| Keys | Does |
|---|---|
| `r` | restore the item |
| `D` | delete it permanently |
| `X` | empty the whole trash |

Neither can be undone, so lazybw asks first. For emptying the trash you
confirm by typing `empty`.

## Undo and redo

`u` undoes the last change to the vault and `Ctrl-r` redoes it. This covers
creating, editing, trashing, restoring, favouriting and moving items, and
creating, renaming and deleting folders. A message tells you what was undone,
for example "Undone: moved 3 items to Trash".

lazybw keeps the last 100 changes of the session. The history is cleared when
the vault locks. Permanent deletion, emptying the trash, import and export
cannot be undone.

One detail about passwords: if you undo a password change, the item gets the
old password back, but Bitwarden's password history keeps the newer one.

## Working on several items

There are two ways to act on more than one item.

`x` marks or unmarks the item under the cursor. Mark as many as you like,
anywhere in the list, and the next action applies to all of them. The marks
are cleared once it has run, and visual mode ends the same way.

`V` starts visual mode, as in Vim. Moving with `j`, `k`, `G` and the rest
extends the selection. Then:

| Keys | Does to the selection |
|---|---|
| `d` | move to the trash |
| `m` | move to a folder |
| `*` | make favourites (or remove them, if all already are) |
| `D` / `r` | delete permanently / restore (in the trash) |
| `Esc` or `V` | leave visual mode |

A bulk action is one step in the undo history, so one `u` undoes all of it.

## Favourites and copies

`*` adds the item to your favourites or removes it. `Y` copies the whole
item and `p` creates a new item from that copy, which is a quick way to make
a similar login.
