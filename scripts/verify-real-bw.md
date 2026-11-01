# Verifying lazybw with the real Bitwarden CLI

Run this checklist before a release, with a real `bw` (≥ 2025.9.0) and
your own account. Use a throwaway account for anything that mutates data if
you can.

Last full run: 2026-11-01, `bw` 2026.10.1, on the maintainer's own vault
(steps 5, 6 and 8 with a disposable item and a one-item export instead of a
throwaway account). All ten steps passed.

## 1. Diagnostics

```sh
lazybw doctor
```

Expected: `bw` path, version ≥ 2025.9.0 and the capabilities
(`archived=… new_item_types=… password_health=…`), the login status, the
clipboard backend, the terminal size, the colour depth and the XDG paths.

## 2. First login

1. Log out first: `bw logout`.
2. `lazybw` → Login screen.
3. Choose the server (bitwarden.com, bitwarden.eu or self-hosted), then
   "Email and password".
4. The TUI is released and `bw login` runs in the terminal with the 2FA
   prompts.
5. After success the vault opens **without a second password prompt**.

## 3. Unlock and compare

1. Quit (`q`), relaunch.
2. The Unlock screen appears; enter the master password.
3. The item counts per type and folder match the web vault.

## 4. No listening socket

```sh
ss -xlpn | grep bw
ss -tlpn | grep bw
```

Expected: no socket and no TCP port owned by `bw`. lazybw uses a socketpair on
fd 3.

## 5. Mutations

Create an item, add a URI and a hidden field, favourite it, move it to a
folder, move it to the trash, press `u` to undo, then restore it. Press `R` and
check the web vault.

Expected: every change is present in the web vault after the sync.

## 6. Passkeys survive edits

Open an item that has a passkey, edit its notes, save. Check the passkey in the
web vault.

Expected: the passkey still exists.

## 7. Exports

```
:export json ~/lazybw-check.json
:export encrypted ~/lazybw-check.enc.json
```

Expected: `ls -l` shows `-rw-------` for both. Re-importing the protected file
into a test account works.

## 8. Duplicate-aware import

Import a Bitwarden JSON that contains duplicates of existing items.

Expected: the preview lists them, "skip duplicates" is on by default and the
import result reports how many were skipped.

## 9. Clipboard

Copy a password (`yp`). After 30 s the clipboard is empty. Check
`cliphist`/`klipper`: the value is not retained (where the platform supports
the password hint).

## 10. Crash safety

```sh
pkill -9 lazybw
pgrep -af "bw serve"
```

Expected: no `bw serve` process remains (PR_SET_PDEATHSIG + kill-on-drop).
