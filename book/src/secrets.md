# Reading and copying secrets

## The detail pane

The right pane shows every field of the selected item: username, password,
verification code, websites, custom fields, notes and, for cards and
identities, their own fields. Passwords and hidden fields show as dots.

Press `l` or `Enter` on an item to move into the detail pane. There `j` and
`k` move from field to field, and the hint on the right of each row tells you
the key that acts on it.

## Copying

| Keys | Copies |
|---|---|
| `yy` | the main field (in the list) or the field under the cursor (in the detail pane) |
| `yu` | the username |
| `yp` | the password |
| `yt` | the current verification code |
| `yU` | the first website |
| `yn` | the notes |

Copying never shows the value on screen. The status line counts down while
the value sits on the clipboard, and after 30 seconds lazybw clears the
clipboard, but only if it still holds what lazybw put there. If you copied
something else in the meantime, lazybw leaves it alone. Change the delay with
`security.clipboard_timeout`.

lazybw uses the system clipboard on X11 and Wayland, falling back to
`wl-copy`, `xclip` or `xsel`. Over SSH, without a display, it sends the text
to your local terminal with the OSC 52 escape sequence, which most modern
terminal emulators accept.

## Revealing a value

`za` shows the hidden field under the cursor, or the password when you are in
the list. `zR` shows every hidden field of the item and `zM` hides them all
again. A revealed value hides itself after 20 seconds
(`security.reveal_timeout`), when you move to another item, and when the
terminal loses focus.

## Verification codes

If a login has an authenticator key, lazybw computes the six-digit code
itself and shows it with a bar that empties as the 30-second window runs
out. This works without a Bitwarden Premium subscription, because lazybw
never asks the server for the code. `yt` copies it. Turn the codes off with
`security.totp = false`.

## Items that ask for the master password

Bitwarden lets you mark an item so the master password is required before
anything secret in it is shown. lazybw honours this flag everywhere: before
revealing, copying, editing, opening the password history or generating a
new password for such an item, it asks for your master password. A wrong
password keeps the dialog open so you can try again; `Esc` or `Ctrl-c`
cancels.

To skip the question for a while after a correct answer, set
`security.reprompt_grace`, for example to `"5m"`.

## Opening a website

`gx` opens the item's first website in your browser. In the detail pane it
opens the website under the cursor. A website saved without a scheme, such
as `github.com`, opens as `https://github.com`.

Links that are not web addresses (`file:`, `androidapp:`, or a scheme some
program registered, like `vscode:`) ask before opening, because the item may
come from a shared collection and the link starts whatever program handles
it.

## Password history and item info

`gh` lists the item's previous passwords with the date each one stopped being
used. `K` shows the item's details: when it was created, edited and when the
password last changed, its id, folder and type.
