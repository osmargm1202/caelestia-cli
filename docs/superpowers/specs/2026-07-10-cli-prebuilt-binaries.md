# Caelestia-cli prebuilt binaries

**Status:** implemented (Phase A+B). Phase C+D (docs + Nix repo handoff) pending.

## Context

Compiling the Rust workspace from source on every NixOS host via `nh os switch`
takes several minutes per host because each Nix derivation rebuilds the full
toolchain chain. This spec describes how to consume prebuilt `caelestia`
binaries that the upstream release workflow publishes on GitHub.

## How it works

```
[tags push]
  ↓
.github/workflows/release.yml
  ↓
cargo build --release --target x86_64-unknown-linux-gnu
  ↓
stage: caelestia + completions/caelestia.fish
  ↓
tar.gz + sha256
  ↓
softprops/action-gh-release@v2
  ↓
github release tag `vX.Y.Z`
  ↓
caelestia-cli/nixos/packages/cli.nix (fetchurl)
  ↓
binary lands in `/nix/store/<hash>-caelestia-cli-X.Y.Z/`
```

The downstream flakes (`~Hobby/nixos`, `osmargm1202/shell`) consume
`caelestia-cli-bin` instead of compiling locally; `nh os switch` reuses
`/nix/store` cache for unchanged tags.

## Maintaining the version pin

Bumping a consumer's pinned CLI release is a 1-line edit:

```diff
  caelestia-cli-bin = pkgs.callPackage ./nixos/packages/cli.nix {
    caelestia-shell = inputs.caelestia-shell.packages.${pkgs.system}.default;
-   version = "v0.4.2";
-   sha256 = "0000000000000000000000000000000000000000000000000000000000000000";
+   version = "v0.4.3";
+   sha256 = "<sha256 published in v0.4.3's release notes>";
  };
```

The checksums are published as `cli-x86_64-linux.sha256` alongside the
tarball on each GitHub release.

The intent is to roll this pin-bump step into an automated workflow
(Phase D of the roadmap): `release.yml` writes the new sha256 into
`flake.nix` automatically. Until that ships, the workflow keeps the placeholder
sha256 of zeros so a forgotten bump fails loudly at evaluation time instead of
silently serving stale binaries.

## Cutting a release

```sh
git tag -a v0.4.3 -m "Phase 4: wallpaper subcommand"
git push origin v0.4.3
```

`release.yml` runs:
1. `cargo build --release --target x86_64-unknown-linux-gnu` on `ubuntu-latest`.
2. Tarballs the binary + `completions/caelestia.fish`.
3. Computes the sha256.
4. Publishes both via `softprops/action-gh-release@v2`.

Downstream flakes need a `nix flake update` to pick up the new tag before
the next `nh os switch`.

## Versioning policy

`version` is `vMAJOR.MINOR.PATCH` to match `git tag -a`. Major bumps trigger
manual review of the pin (compatibility notes) before being merged.

## Security notes

- `sha256` placeholder is intentional; the build will fail without the
  caller-provided hash so a forgotten bump is loud.
- Prebuilt binaries run inside the same sandbox as the source tree — they
  are not "trusted sources". Use this approach for convenience, not for
  replacing supply-chain reviews.
- The shell `qs -c caelestia ipc call launcher openClipboard` delegation
  is unchanged; both source-built and prebuilt binaries use the same exec
  path.

## References

- `caelestia-cli/nix development shell`: `cargo build --release`.
- `~Hobby/nixos/flake.nix`: downstream consumer of `caelestia-cli-bin`.
- `~Hobby/nixos/nixos/profiles/hyprlandqs-caelestia.nix`: profile that
  consumes `caelestia-shell.packages.<sys>.with-cli`.
