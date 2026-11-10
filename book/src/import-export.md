# Import and export

## Export

`Space` `e` (or `:export`) opens the export wizard.

1. Choose the format and where to write the file. The wizard suggests the
   password-protected format, the safe choice for a backup. `Tab` completes
   the path, and the file name follows the format (`.json` or `.csv`). If
   the file exists, the wizard says so and replaces it only on a second
   `Enter`.
2. For a password-protected file, choose its password and type it again. A
   strength bar shows how good it is.
3. Confirm with your master password.

The formats:

| Format | Notes |
|---|---|
| JSON | readable, every field, not encrypted |
| JSON encrypted with a password | for backups; any Bitwarden account can import it with the password |
| JSON encrypted with your account | only this Bitwarden account can import it |
| CSV | logins and notes only, not encrypted |

The file is created readable only by you (mode 600). Plain JSON and CSV
exports hold every password in clear text, and the wizard says so before
writing. Keep such a file on an encrypted disk and delete it when you are
done.

You can skip the first step with `:export json ~/backup.json`, or with
`encrypted`, `account` or `csv` as the format.

## Import

`Space` `i` (or `:import`) opens the import wizard.

1. Pick the format. The list comes from `bw import --formats`, with the
   Bitwarden formats first. Type to filter it: `1password`, `keepass`,
   `lastpass` and many others are there.
2. Type the path of the file. `Tab` completes it. A password-protected
   Bitwarden file also asks for its password.
3. For Bitwarden JSON files, lazybw shows a preview before importing.

A large import or export can take a while. `Esc` closes the wizard without
stopping it, and lazybw says when it is done. Locking the vault stops an
export, but not an import, which would otherwise be left half done.

The preview counts the items by type, lists the folders the import will
create, and finds items you already have. An item counts as a duplicate when
it has the same type and name and, for logins, the same username and the same
site. Duplicates are skipped by default. In the preview:

| Keys | Does |
|---|---|
| `j` / `k` | move through the duplicates |
| `x` or `Space` | keep this duplicate (import it), or skip it again |
| `a` | skip all duplicates, or none |
| `s` | turn skipping off (import everything) or on |
| `Enter` | import |

When skipping, lazybw writes a filtered copy of the file to
`$XDG_RUNTIME_DIR/lazybw`, imports that, and overwrites and deletes the copy
straight away. Folders of the remaining items are kept.

After the import, lazybw syncs, shows "Imported N items. Skipped M
duplicate(s).", and lists all items sorted by creation date, so the new ones
are easy to check.

Imports cannot be undone, so it is worth exporting a backup first.

`:import bitwardenjson ~/file.json` skips straight to the file step.
