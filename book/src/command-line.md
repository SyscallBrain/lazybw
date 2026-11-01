# The command line

`:` opens a command line at the bottom of the screen, as in Vim. `Tab`
completes commands and their arguments (themes, folders, item types, settings,
file paths), and the up and down arrows go through earlier commands. lazybw
keeps this history between sessions.

The most useful commands:

| Command | Does |
|---|---|
| `:q`, `:q!` | quit (in the editor: close the item, or discard it) |
| `:qa`, `:qa!` | quit, even from the editor |
| `:w`, `:wq` | save, save and close (in the editor) |
| `:new login` | new item of a type (`login`, `note`, `card`, `identity`, `ssh`) |
| `:edit json` | edit the item as JSON in `$EDITOR` |
| `:copy password` | copy a field of the selected item |
| `:move Work` | move the selected item to a folder |
| `:folder new Taxes` | create a folder (also `rename` and `delete`) |
| `:gen 32`, `:gen phrase 6` | generate a password or passphrase and copy it |
| `:sort modified`, `:sort name!` | change the sort order (`!` reverses it) |
| `:theme nord` | switch theme |
| `:set theme=nord` | change a setting; `:set animations!` toggles one |
| `:sync`, `:sync!` | sync, or force a full sync |
| `:lock`, `:logout` | lock the vault, log out |
| `:export`, `:import` | open the wizards |
| `:health`, `:trash`, `:empty-trash` | open the health tab, the trash, empty the trash |
| `:12` | go to the twelfth item |
| `:doctor` | show the same diagnostics as `lazybw doctor` |

The [default keybindings](reference/keymap.md#command-line) page has the
complete list.
