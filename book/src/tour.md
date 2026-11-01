# A tour of the screen

The vault screen has a bar at the top, panes in the middle and two lines at
the bottom.

```text
 ◆ lazybw  1 Vault  2 Generator  3 Health       you@example.com │ unlocked
╭ Filters ──────────╮╭ Logins 198 ──── name ↓ ╮╭ GitHub ──────────── Login ╮
│  ≡ All items  247 ││ ● Amazon               ││ ● GitHub                  │
│  ★ Favorites   12 ││   shop@example.com     ││ Login · Personal          │
│ TYPES             ││▌● GitHub           ★ ◷ ││ USERNAME                  │
│▌ ● Logins     198 ││▌  dev-example          ││ dev-example            yu │
│ FOLDERS           ││ ● GitLab            ◷  ││ PASSWORD                  │
│  ▾ Personal    80 ││   dev@example.com      ││ •••••••••• za reveal · yp │
│      Banking    6 ││                        ││                           │
╰───────────────────╯╰────────────────────────╯╰───────────────────────────╯
 NORMAL  Vault › Logins › GitHub          ⟳ 2m ago │ ⚿ locks 14:32   3/198
 Copied password for "GitHub". Clipboard clears in 30s.
```

## The top bar

On the left are the three tabs: Vault, Generator and Health. Switch with
`Space` followed by `1`, `2` or `3`, or with `gt` and `gT`. On the right you
see the account email, the server and whether the vault is unlocked.

## The panes

The filters pane lists All items, Favorites, the item types, your folders,
No folder and the Trash, each with a count. The middle pane lists the items
that match the selected filter. The right pane shows the item under the
cursor.

The focused pane has a coloured border. `Tab` moves the focus to the next
pane, and `h` and `l` move left and right.

The layout follows the width of the terminal:

| Width | Layout |
|---|---|
| 140 columns or more | Three panes, as above. |
| 100 to 139 columns | The filters become a narrow column of icons (the rail). The folder icon opens a folder picker. |
| Under 100 columns | One pane at a time. `l` or `Enter` goes one level deeper, `h` goes back. |

`Space` `b` hides or shows the filters pane, and you can drag the pane
borders with the mouse to resize them.

## The bottom lines

The status line starts with the mode (NORMAL, INSERT, VISUAL, COMMAND or
SEARCH), followed by where you are. Further right it shows the keys you have
typed so far, the clipboard countdown while a copied value is pending, the
time of the last sync, when the vault will lock, and your position in the
list.

The line under it shows messages, such as "Copied password" or an error from
`bw`. `Space` `m` opens the history of all messages from the session.

## List markers

Items in the list can carry small markers on the right: `★` for favourites,
`◷` for items with a verification code, an icon for passkeys and a lock for
items that ask for the master password again. After a health check, a
warning sign marks passwords that are weak, reused or exposed.
