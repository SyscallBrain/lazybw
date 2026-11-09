# Command-line options

```text
lazybw                          start the interface
lazybw --version                print the version
lazybw --help                   print help
```

## `lazybw doctor`

Prints what lazybw found: the `bw` path, its version and the features it
enables, the login state, the clipboard backend, the terminal size and colour
depth, and the configuration, state and log paths. Inside tmux it also
reminds you about `extended-keys`.

## `lazybw keys`

```text
lazybw keys                     every binding, grouped by context
lazybw keys --context list      only one context
lazybw keys --defaults          the built-in defaults, ignoring your config
lazybw keys --check             validate the bindings; exit code 1 on errors
```

The columns are the context, the keys, the action, its description and where
the binding comes from (`default` or `user`).

## `lazybw themes`

```text
lazybw themes                   list the built-in themes
lazybw themes --preview nord    print the colours of one theme
```

## `lazybw config`

```text
lazybw config --path            print the path of config.toml
lazybw config --print-default   print the default configuration
lazybw config --check           validate config.toml; exit code 1 on errors
```

## Environment variables

| Variable | Effect |
|---|---|
| `LAZYBW_BW` | path to `bw`, used when `bw.path` is empty |
| `LAZYBW_LOG` | how much goes into the log: `info` (the default), `debug`, `trace`, `warn`, `error` or `off` |
| `BW_SESSION` | reused only with `bw.inherit_session = true` |
| `BITWARDENCLI_APPDATA_DIR` | passed on to `bw` (its data directory) |
| `VISUAL`, `EDITOR` | the editor for `Ctrl-e` and `gE`, unless `editor.command` is set |
| `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, `XDG_RUNTIME_DIR` | where lazybw keeps its files |
| `COLORTERM` | `truecolor` or `24bit` turns on 24-bit colour |
| `NO_COLOR` | turns colours off |

## Signals

`SIGUSR1` locks the vault (`pkill -USR1 lazybw`). `SIGTERM` and `SIGHUP` quit
cleanly.
