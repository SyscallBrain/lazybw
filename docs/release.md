# Release checklist

1. `scripts/check.sh` is green (fmt, clippy `-D warnings`, every test).
2. `docs/design.md` and `book/src/reference/keymap.md` match the
   implementation; `book/src/reference/config.md` contains
   `crates/lazybw/assets/default-config.toml` (`tests/docs.rs` enforces it).
3. `CHANGELOG.md` has the release section with the date.
4. The new version is in the workspace `Cargo.toml` (and `Cargo.lock`), in
   `packaging/aur/*/PKGBUILD` and in the file names of
   `book/src/installation.md`. The README's crates.io installation command
   stays `cargo install --locked lazybw`.
5. `cargo publish -p lazybw --dry-run --locked` succeeds. Inspect
   `cargo package -p lazybw --list` to confirm the tarball contains the
   embedded themes and default config.
6. `cargo run -p lazybw --example screenshots` regenerates
   `crates/lazybw/assets/screenshots/*.svg` and the copies in the book; the
   README links resolve and `mdbook build book` shows no warnings.
7. Before publishing anything, build the candidate with `scripts/dist.sh`
   and try it by hand with a real `bw` and a dedicated test account. Check
   login/unlock, both private transports, sync, create/edit/trash/restore,
   clipboard clearing and locking. Record the lazybw revision, `bw` version
   and results in the release PR. Use sample items, never a personal vault.
   The automated compatibility check below does not cover these operations.
8. Write `.github/release-notes/vX.Y.Z.md`, tag `vX.Y.Z` and push the tag.
   The Release workflow builds static binaries for x86_64 and aarch64, the
   `.tar.gz`, `.deb`, `.rpm` and Arch packages, smoke-tests them and publishes
   the GitHub release; the Docker workflow publishes
   `ghcr.io/syscallbrain/lazybw:X.Y.Z` and `:latest`. `scripts/dist.sh`
   builds the same files locally.
9. After the GitHub release succeeds, publish with
   `cargo publish -p lazybw --locked`. Confirm the crates.io version and
   install it into a temporary prefix with
   `cargo install --locked --version X.Y.Z --root /tmp/lazybw-X.Y.Z lazybw`;
   check `/tmp/lazybw-X.Y.Z/bin/lazybw --version`.
10. Publish the AUR packages (below) once the GitHub release exists.

## Official Bitwarden CLI compatibility

CI and the release workflow both require checks against official npm
`@bitwarden/cli` versions 2025.9.0 (minimum) and 2026.10.1 (the Docker pin).
Keep the compatibility matrix and Docker pin aligned when updating `bw`.
For a local run with an already installed official CLI:

```sh
python3 scripts/check-bw.py /absolute/path/to/bw 2026.10.1
```

The runner builds the opt-in `real_bw` test and then runs it in a disposable
HOME, XDG directories and `BITWARDENCLI_APPDATA_DIR`, with no inherited
Bitwarden credentials or lazybw settings. It checks the exact version,
parses a successful logged-out `bw status` through lazybw's subprocess
client, and checks that both `bw serve` transports produce the expected
`NotLoggedIn` error. Any other error or timeout fails the check. Both
versions refuse to start the HTTP server without an account, so this does
not verify HTTP requests over an authenticated connection, live login,
sync or vault CRUD. Those remain the pre-release manual check above.

`python3 -m unittest discover -s scripts -p test_check_bw.py` checks that the
runner isolates inherited credentials and cleans up even when its child
fails. The regular test suite continues to use `fake-bw`.

## AUR

Two packages: `lazybw` builds from the tag's source tarball, `lazybw-bin`
installs the static binaries of the release. Their PKGBUILDs live in
`packaging/aur/`; `scripts/aur.sh` turns them into AUR-ready repositories
with the release's checksums and `.SRCINFO`.

### Once per machine

1. Add your SSH public key (`~/.ssh/id_ed25519.pub`) in your AUR account:
   <https://aur.archlinux.org>, My Account, SSH Public Key.
2. Tell SSH to use it for the AUR, in `~/.ssh/config`:

   ```text
   Host aur.archlinux.org
     IdentityFile ~/.ssh/id_ed25519
     User aur
   ```

3. Check: `ssh aur@aur.archlinux.org help` lists the AUR commands.

### For each release

```sh
scripts/aur.sh X.Y.Z ~/aur        # clones ~/aur/lazybw and ~/aur/lazybw-bin if needed
cd ~/aur/lazybw-bin && makepkg -si && namcap PKGBUILD ./*.pkg.tar.zst
cd ~/aur/lazybw     && makepkg -si && namcap PKGBUILD ./*.pkg.tar.zst
```

If both install and run, publish each one:

```sh
git add PKGBUILD .SRCINFO .gitignore
git commit -m "Update to X.Y.Z"
git push
```

The first push to a name creates the package; the AUR only accepts the
`master` branch (the script sets it). For a packaging fix without a new
release, raise `pkgrel` in the AUR repository's PKGBUILD, regenerate
`.SRCINFO` with `makepkg --printsrcinfo > .SRCINFO`, and copy the change back
to `packaging/aur/`. `scripts/aur.sh X.Y.Z /tmp/aur --local` tries both
packages before a release, from `git archive HEAD` and `dist/`.
