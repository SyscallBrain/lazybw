# Release checklist

1. `scripts/check.sh` is green (fmt, clippy `-D warnings`, every test).
2. `docs/design.md` and `book/src/reference/keymap.md` match the
   implementation; `book/src/reference/config.md` contains
   `crates/lazybw/assets/default-config.toml` (`tests/docs.rs` enforces it).
3. `CHANGELOG.md` has the release section with the date.
4. The new version is in the workspace `Cargo.toml` (and `Cargo.lock`), in
   `packaging/aur/*/PKGBUILD`, in the `cargo install --tag` line of the README
   and in the file names of `book/src/installation.md`.
5. `cargo package -p lazybw --no-verify` succeeds and the tarball contains the
   embedded themes and default config.
6. `cargo run -p lazybw --example screenshots` regenerates
   `crates/lazybw/assets/screenshots/*.svg` and the copies in the book; the
   README links resolve and `mdbook build book` shows no warnings.
7. Write `.github/release-notes/vX.Y.Z.md`, tag `vX.Y.Z` and push the tag.
   The Release workflow builds static binaries for x86_64 and aarch64, the
   `.tar.gz`, `.deb`, `.rpm` and Arch packages, smoke-tests them and publishes
   the GitHub release; the Docker workflow publishes
   `ghcr.io/syscallbrain/lazybw:X.Y.Z` and `:latest`. `scripts/dist.sh`
   builds the same files locally.
8. Try the release by hand with a real `bw` and account (the only step the
   automated suite cannot do).
9. Publish the AUR packages (below) once the GitHub release exists.

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
