# Rust Migration — Phase 1: Scaffolding + Delegation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Python entry point with a Rust binary that delegates every subcommand to the existing Python code, packaged via a rewritten flake — drop-in identical behavior, foundation for incremental porting.

**Architecture:** Python source moves to `python-ref/` (reference implementation). A minimal Rust binary (`src/main.rs`) inspects argv: native subcommands (none yet, the set grows each phase) are handled in Rust; everything else `exec()`s `python3 -m caelestia` with `PYTHONPATH` pointing at `python-ref/src`. The Nix package builds the Rust binary, ships the Python tree + a python env, and wires them via wrapper env vars.

**Tech Stack:** Rust (edition 2021, no external crates this phase), Nix flake (`rustPlatform.buildRustPackage`), Python 3.13 env from nixpkgs (`materialyoucolor`, `pillow`), uv in devshell.

**Design note (deviation from spec §7.1):** The full clap tree is NOT built this phase. Delegated subcommands are matched only by name (first non-flag arg); clap parsing is added per-subcommand as each goes native. Reason: duplicating argparse semantics (filesystem-dependent `choices`, dynamic `const=` defaults) for commands that immediately re-parse in Python would create divergence risk for zero benefit.

**Verification env note:** The current devshell has no cargo/rustc. Until Task 3 lands, run cargo through `nix shell nixpkgs#cargo nixpkgs#rustc --command ...`.

---

### Task 1: Move Python code to `python-ref/`

**Files:**
- Move: `src/` → `python-ref/src/` (whole tree, includes `src/caelestia/data/templates/`)
- Move: `pyproject.toml` → `python-ref/pyproject.toml`
- Create: `python-ref/src/caelestia/__main__.py`

- [ ] **Step 1: Move the tree with git mv**

```bash
cd /home/osmarg/Hobby/caelestia-cli
mkdir python-ref
git mv src python-ref/src
git mv pyproject.toml python-ref/pyproject.toml
```

- [ ] **Step 2: Add `__main__.py` so `python -m caelestia` works**

The package only has a `caelestia:main` script entry point today; `-m` needs `__main__.py`.

Create `python-ref/src/caelestia/__main__.py`:

```python
from caelestia import main

if __name__ == "__main__":
    main()
```

- [ ] **Step 3: Verify the moved package still runs**

```bash
nix shell nixpkgs#python313 nixpkgs#python313Packages.pillow nixpkgs#python313Packages.materialyoucolor \
  --command sh -c 'PYTHONPATH=python-ref/src python3 -m caelestia --help'
```

Expected: the argparse help text (`usage: caelestia [-h] [-v] COMMAND ...`) with all 12 subcommands listed, exit 0.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: move Python implementation to python-ref/

Python becomes the reference implementation during the Rust
migration. Adds __main__.py so the Rust dispatcher can invoke
it via python -m caelestia."
```

---

### Task 2: Rust dispatcher binary

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `tests/delegation.rs`
- Modify: `.gitignore` (create if absent)

- [ ] **Step 1: Create `Cargo.toml`**

```toml
[package]
name = "caelestia"
version = "1.0.0"
edition = "2021"
description = "Main control script for the Caelestia dotfiles (NixOS fork)"
license = "GPL-3.0-only"

[dependencies]
```

- [ ] **Step 2: Add `target/` to `.gitignore`**

```gitignore
target/
result
```

- [ ] **Step 3: Write failing unit tests inside `src/main.rs`**

Create `src/main.rs` with tests first (functions referenced don't exist yet — this won't compile, which is the TDD "red"):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_subcommand_skips_flags() {
        let args: Vec<String> = vec!["-v".into()];
        assert_eq!(first_subcommand(&args), None);

        let args: Vec<String> = vec!["shell".into(), "-d".into()];
        assert_eq!(first_subcommand(&args), Some("shell"));

        let args: Vec<String> = vec!["--version".into(), "toggle".into()];
        assert_eq!(first_subcommand(&args), Some("toggle"));

        let args: Vec<String> = vec![];
        assert_eq!(first_subcommand(&args), None);
    }

    #[test]
    fn no_native_subcommands_in_phase_1() {
        for sub in ["shell", "toggle", "scheme", "screenshot", "record",
                    "clipboard", "emoji", "wallpaper", "resizer", "search",
                    "install", "update"] {
            assert!(!is_native(sub), "{sub} must delegate in phase 1");
        }
    }
}
```

- [ ] **Step 4: Run tests, verify failure**

```bash
nix shell nixpkgs#cargo nixpkgs#rustc --command cargo test
```

Expected: compile error — `first_subcommand` / `is_native` not found.

- [ ] **Step 5: Implement the dispatcher**

Prepend to `src/main.rs` (above the tests module):

```rust
use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

/// Subcommands implemented natively in Rust. Grows each migration phase.
const NATIVE: &[&str] = &[];

fn is_native(subcommand: &str) -> bool {
    NATIVE.contains(&subcommand)
}

/// First non-flag argument = the subcommand name, mirroring how argparse
/// resolves it on the Python side.
fn first_subcommand(args: &[String]) -> Option<&str> {
    args.iter().map(String::as_str).find(|a| !a.starts_with('-'))
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match first_subcommand(&args) {
        Some(sub) if is_native(sub) => unreachable!("no native subcommands yet"),
        _ => delegate(&args),
    }
}

/// Replace this process with the Python reference implementation.
/// exec() (not spawn) so stdin/tty, signals and exit codes pass through
/// untouched — interactive prompts in install/update keep working.
fn delegate(args: &[String]) -> ! {
    let python = env::var("CAELESTIA_PYTHON").unwrap_or_else(|_| "python3".into());
    let pythonpath = env::var("CAELESTIA_PYTHONPATH")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/python-ref/src").into());

    let err = Command::new(python)
        .arg("-m")
        .arg("caelestia")
        .args(args)
        .env("PYTHONPATH", pythonpath)
        .exec();

    eprintln!("caelestia: failed to launch python backend: {err}");
    std::process::exit(1);
}
```

- [ ] **Step 6: Run unit tests, verify pass**

```bash
nix shell nixpkgs#cargo nixpkgs#rustc --command cargo test
```

Expected: `test tests::first_subcommand_skips_flags ... ok`, `test tests::no_native_subcommands_in_phase_1 ... ok`.

- [ ] **Step 7: Write integration test for delegation**

Create `tests/delegation.rs`:

```rust
use std::process::Command;

// CAELESTIA_PYTHON=echo turns the delegation into an observable echo of
// exactly what would be exec'd, without needing a real Python env.

#[test]
fn delegates_full_argv_to_python_backend() {
    let out = Command::new(env!("CARGO_BIN_EXE_caelestia"))
        .args(["scheme", "get", "-n"])
        .env("CAELESTIA_PYTHON", "echo")
        .output()
        .expect("failed to run caelestia binary");

    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.trim(), "-m caelestia scheme get -n");
}

#[test]
fn delegates_when_no_subcommand_given() {
    let out = Command::new(env!("CARGO_BIN_EXE_caelestia"))
        .args(["--version"])
        .env("CAELESTIA_PYTHON", "echo")
        .output()
        .expect("failed to run caelestia binary");

    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.trim(), "-m caelestia --version");
}
```

- [ ] **Step 8: Run all tests, verify pass**

```bash
nix shell nixpkgs#cargo nixpkgs#rustc --command cargo test
```

Expected: 4 tests pass (2 unit + 2 integration). `Cargo.lock` gets generated — it must be committed (Nix build needs it).

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs tests/delegation.rs .gitignore
git commit -m "feat: add Rust dispatcher delegating to Python reference

New entry point: a Rust binary that exec()s python -m caelestia
for every subcommand. The NATIVE set is empty; it grows as
subcommands are ported in later phases."
```

---

### Task 3: Rewrite Nix packaging (flake.nix + default.nix)

**Files:**
- Modify: `flake.nix`
- Modify: `default.nix` (full rewrite)

- [ ] **Step 1: Rewrite `default.nix`**

Replace the entire file with:

```nix
{
  rev,
  lib,
  rustPlatform,
  python3,
  makeWrapper,
  installShellFiles,
  swappy,
  libnotify,
  slurp,
  wl-clipboard,
  cliphist,
  xdg-utils,
  dart-sass,
  grim,
  fuzzel,
  gpu-screen-recorder,
  dconf,
  killall,
  ffmpeg,
  caelestia-shell,
  withShell ? false,
  discordBin ? "discord",
  qtctStyle ? "Darkly",
}: let
  pythonEnv = python3.withPackages (ps: [
    ps.materialyoucolor
    ps.pillow
  ]);

  runtimeDeps =
    [
      swappy
      libnotify
      slurp
      wl-clipboard
      cliphist
      xdg-utils
      dart-sass
      grim
      fuzzel
      gpu-screen-recorder
      dconf
      killall
      ffmpeg
    ]
    ++ lib.optional withShell caelestia-shell;
in
  rustPlatform.buildRustPackage {
    pname = "caelestia-cli";
    version = "${rev}";
    src = ./.;

    cargoLock.lockFile = ./Cargo.lock;

    nativeBuildInputs = [makeWrapper installShellFiles];

    # Same substitutions as the old buildPythonApplication patchPhase,
    # retargeted at python-ref/. They die with python-ref in phase 6.
    postPatch = ''
      substituteInPlace python-ref/src/caelestia/subcommands/shell.py \
        --replace-fail '"qs", "-c", "caelestia"' '"caelestia-shell"'
      substituteInPlace python-ref/src/caelestia/subcommands/screenshot.py \
        --replace-fail '"qs", "-c", "caelestia"' '"caelestia-shell"'

      substituteInPlace python-ref/src/caelestia/subcommands/toggle.py \
        --replace-fail 'discord' '${discordBin}' \
        --replace-fail '["todoist"]' '["todoist.desktop"]'

      substituteInPlace python-ref/src/caelestia/data/templates/qtengine.json \
        --replace-fail 'Darkly' '${qtctStyle}'
    '';

    postInstall = ''
      mkdir -p $out/share/caelestia/python
      cp -r python-ref/src/caelestia $out/share/caelestia/python/

      installShellCompletion completions/caelestia.fish
    '';

    postFixup = ''
      wrapProgram $out/bin/caelestia \
        --set CAELESTIA_PYTHON ${pythonEnv}/bin/python3 \
        --set CAELESTIA_PYTHONPATH $out/share/caelestia/python \
        --prefix PATH : ${lib.makeBinPath runtimeDeps}
    '';

    meta = {
      description = "The main control script for the Caelestia dotfiles (NixOS fork, Rust)";
      homepage = "https://github.com/osmargm1202/caelestia-cli";
      license = lib.licenses.gpl3Only;
      mainProgram = "caelestia";
      platforms = lib.platforms.linux;
    };
  }
```

- [ ] **Step 2: Rewrite the devShell in `flake.nix`**

Replace only the `devShells` attribute (packages/formatter stay as-is):

```nix
    devShells = forAllSystems (pkgs: {
      default = pkgs.mkShell {
        packages = with pkgs; [
          cargo
          rustc
          rustfmt
          clippy
          rust-analyzer
          uv
          (python3.withPackages (ps: [ps.materialyoucolor ps.pillow]))
          alejandra
        ];
      };
    });
```

- [ ] **Step 3: Build and smoke-test the package**

```bash
git add -A   # nix flake builds only see tracked/staged files
nix build
./result/bin/caelestia --help
```

Expected: `nix build` succeeds (runs `cargo test` in checkPhase); `--help` prints the argparse help with all 12 subcommands — proof the Rust→Python delegation works with the wrapped env vars.

- [ ] **Step 4: Smoke-test a real delegated subcommand**

```bash
./result/bin/caelestia scheme list -n
```

Expected: list of scheme names (or the same output the current Python CLI gives on this machine), exit 0.

- [ ] **Step 5: Verify devshell**

```bash
nix develop --command cargo test
nix develop --command sh -c 'PYTHONPATH=python-ref/src python3 -c "import caelestia; print(\"ok\")"'
```

Expected: 4 cargo tests pass; `ok`.

- [ ] **Step 6: Commit**

```bash
git add flake.nix default.nix
git commit -m "build: package Rust dispatcher via rustPlatform

default.nix builds the Rust binary, ships python-ref as the
delegated backend (wrapped python env + PYTHONPATH), and keeps
the same runtime binaries and patchPhase substitutions.
Devshell gains the Rust toolchain and uv."
```

---

### Task 4: README migration notes

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add a migration section to README.md**

Append after the intro section:

```markdown
## Rust migration (NixOS fork)

This fork is being migrated from Python to Rust
(see `docs/superpowers/specs/2026-07-10-rust-migration-design.md`).

The `caelestia` binary is Rust; subcommands not yet ported are
delegated transparently to the Python reference implementation in
`python-ref/`. Behavior is drop-in identical.

`install` and `update` are Arch-specific and will become stubs — on
NixOS, dependencies are managed by the flake.

### Runtime dependencies

Provided automatically by the Nix package: swappy, libnotify, slurp,
wl-clipboard, cliphist, xdg-utils, dart-sass, grim, fuzzel,
gpu-screen-recorder, dconf, killall, ffmpeg (and optionally
caelestia-shell via the `with-shell` package).
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: document Rust migration status and runtime deps"
```

---

### Task 5: Final verification

- [ ] **Step 1: Full clean verification**

```bash
nix build
nix develop --command cargo test
nix develop --command cargo clippy -- -D warnings
./result/bin/caelestia --help
./result/bin/caelestia scheme get -n
```

Expected: build OK, 4 tests pass, clippy clean, help + current scheme name printed.

- [ ] **Step 2: Optional live smoke (user's Hyprland session)**

```bash
./result/bin/caelestia toggle --help
./result/bin/caelestia wallpaper -p
```

Expected: identical output to the pre-migration CLI.

---

## Phase exit criteria

- `nix build` produces a Rust `caelestia` binary whose observable behavior is byte-identical to the Python CLI for all 12 subcommands (all delegated).
- `cargo test` green, `clippy` clean.
- Devshell has rust toolchain + python env + uv for the golden tests of phase 3.

**Next:** Phase 2 plan (util core + trivial subcommands native) gets written once this phase is merged.
