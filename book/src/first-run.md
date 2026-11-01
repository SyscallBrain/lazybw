# First run

Start lazybw with:

```sh
lazybw
```

The first screen says "Starting Bitwarden CLI…" while lazybw finds `bw`,
checks its version and asks it whether you are logged in. What comes next
depends on the answer.

## Logging in

If `bw` has no account yet, you get the login screen. It has two boxes:

- Server: `bitwarden.com` (United States), `bitwarden.eu` (European Union) or
  your own self-hosted server.
- Method: email and password, single sign-on, or an API key.

Move with `j` and `k`, switch between the boxes with `Tab` and press `Enter`
to continue.

For email and password, and for single sign-on, lazybw steps aside and runs
`bw login` in the terminal, so you see Bitwarden's own prompts, including
two-step login. When `bw login` finishes, lazybw comes back and opens the
vault. You are not asked for the master password a second time.

For an API key, lazybw asks for the client id and secret in its own form.
You will find both in the Bitwarden web vault under Settings, Security, Keys.

## Unlocking

When `bw` already knows your account but the vault is locked, lazybw shows
the unlock screen. Type the master password and press `Enter`.

`Ctrl-o` logs out instead (after confirming), and `Ctrl-c` quits. `Ctrl-c`
also quits from the login screen and from the start-up and error screens.

## Locking

The vault locks after 15 minutes without a key press, and you can lock it
yourself with `Space` then `l`. Locking wipes everything lazybw holds in
memory: the decrypted items, the undo history, the generator history and the
clipboard, if it still holds something lazybw copied.

You can also lock a running lazybw from outside, for example from a
screen-locker hook:

```sh
pkill -USR1 lazybw
```

## Quitting

Press `q` on the vault screen, or type `:q`. lazybw clears the clipboard on
the way out if it still holds a value it copied.
