# Packaging

Reference copies of the package definitions. The AUR keeps one git repository
per package, so these are copied over there rather than being built from here.

## AUR

`binkam-bin` installs the binary from the GitHub release; `binkam` builds the
tagged source and runs the tests on the way. Both put `binkam` in
`/usr/bin`, so they conflict with each other on purpose.

Publishing one, the first time:

```bash
# An AUR account with an SSH key is needed; see
# https://wiki.archlinux.org/title/AUR_submission_guidelines
git clone ssh://aur@aur.archlinux.org/binkam-bin.git
cd binkam-bin
cp /path/to/binkam/packaging/aur/binkam-bin/PKGBUILD .

updpkgsums                              # fill in the checksums
makepkg --printsrcinfo > .SRCINFO       # the AUR reads this, not the PKGBUILD
makepkg -si                             # build and install it, as a check
namcap PKGBUILD *.pkg.tar.zst           # and lint both

git add PKGBUILD .SRCINFO
git commit -m "Initial import of binkam-bin 0.2.0"
git push
```

For every later version, bump `pkgver`, reset `pkgrel=1`, and run `updpkgsums`
and `makepkg --printsrcinfo > .SRCINFO` again before pushing.

`updpkgsums` and `namcap` come from `pacman-contrib` and `namcap`.
