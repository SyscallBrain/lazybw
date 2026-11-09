# Installation

## Requirements

- Linux. lazybw works in any terminal emulator, under X11 or Wayland, and over
  SSH.
- The official Bitwarden CLI, `bw`, version 2025.9.0 or newer.
- A recent Rust toolchain (1.88 or newer) to build lazybw.
- A [Nerd Font](https://www.nerdfonts.com) in your terminal, for the icons.
  This one is optional: lazybw can use plain Unicode or ASCII symbols instead.

## Install the Bitwarden CLI

Use your distribution's package if it has one. On Arch Linux:

```sh
sudo pacman -S bitwarden-cli
```

Anywhere else with Node.js installed:

```sh
npm install -g @bitwarden/cli
```

Check the version:

```sh
bw --version
```

lazybw refuses to start with a `bw` older than 2025.9.0, because it needs
the private-socket mode of `bw serve` that appeared in that release. A few
features need newer versions: archived items appear from 2025.10.0, and the
vault health report uses `bw report password-health` from 2026.8.0 (older
versions get a local check without the data-breach lookups).

## Install lazybw

Each [release](https://github.com/SyscallBrain/lazybw/releases/latest) has
static binaries for x86_64 and aarch64 (64-bit ARM), and packages built from
them. The binaries have no library dependencies, so they run on any Linux
distribution.

### Debian and Ubuntu

```sh
sudo apt install ./lazybw_1.1.0-1_amd64.deb     # or _arm64.deb
```

### Fedora, openSUSE and RHEL

```sh
sudo dnf install ./lazybw-1.1.0-1.x86_64.rpm    # or .aarch64.rpm
```

### Arch Linux

lazybw is in the AUR in two forms: `lazybw-bin` installs the release binary,
and `lazybw` builds it from source. With an AUR helper such as `paru` or
`yay`:

```sh
paru -S lazybw-bin     # or: paru -S lazybw
```

Without a helper:

```sh
git clone https://aur.archlinux.org/lazybw-bin.git
cd lazybw-bin
makepkg -si
```

The release also has a ready-made package:

```sh
sudo pacman -U lazybw-1.1.0-1-x86_64.pkg.tar.zst
```

On x86_64 every package pulls in `bitwarden-cli` from the Arch
repositories.

### Any other Linux

```sh
tar -xzf lazybw-1.1.0-x86_64-unknown-linux-musl.tar.gz
install -Dm755 lazybw-1.1.0-x86_64-unknown-linux-musl/lazybw ~/.local/bin/lazybw
```

Every release also has a `SHA256SUMS` file. Check your download with
`sha256sum --check --ignore-missing SHA256SUMS`.

### Android (Termux)

The aarch64 binary runs in [Termux](https://termux.dev):

```sh
pkg install nodejs
npm install -g @bitwarden/cli
tar -xzf lazybw-1.1.0-aarch64-unknown-linux-musl.tar.gz
install -Dm755 lazybw-1.1.0-aarch64-unknown-linux-musl/lazybw "$PREFIX/bin/lazybw"
```

Termux has no system clipboard that lazybw can reach directly; it copies
through your terminal with OSC 52 instead, which the Termux app supports.

### Docker

The image on the GitHub Container Registry includes `bw`:

```sh
docker run -it --rm -v lazybw:/home/lazybw ghcr.io/syscallbrain/lazybw
```

The `lazybw` volume keeps your Bitwarden login, the lazybw configuration and
its state between runs. Inside a container there is no system clipboard;
lazybw copies through your terminal with OSC 52. Run `lazybw` subcommands
by adding them at the end, for example `… ghcr.io/syscallbrain/lazybw doctor`.

### From source

With Rust 1.88 or newer:

```sh
cargo install --git https://github.com/SyscallBrain/lazybw lazybw
```

Or from a clone, which also lets you build the packages yourself
(`scripts/dist.sh`):

```sh
git clone https://github.com/SyscallBrain/lazybw
cd lazybw
cargo install --path crates/lazybw
```

On Arch Linux, the `lazybw` AUR package does this for you.

## Check the setup

```sh
lazybw doctor
```

`doctor` prints what lazybw found: where `bw` is and which version, whether
you are logged in, which clipboard backend it will use, the terminal size and
colour depth, and the paths of the configuration, state and log files. Run it
first whenever something does not work.

## Where lazybw keeps its files

| What | Where |
|---|---|
| Configuration | `~/.config/lazybw/config.toml` |
| Your themes | `~/.config/lazybw/themes/*.toml` |
| Log, command history, folder state | `~/.local/state/lazybw/` |
| Temporary files (exports, `$EDITOR` buffers) | `$XDG_RUNTIME_DIR/lazybw/` |

lazybw follows the XDG variables, so `$XDG_CONFIG_HOME` and `$XDG_STATE_HOME`
move these if you set them. Your vault itself stays where `bw` keeps it.
