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

      # Same substitutions for native Rust subcommands.
      substituteInPlace src/subcommands/shell.rs \
        --replace-fail '"qs", "-c", "caelestia"' '"caelestia-shell"'
      substituteInPlace src/subcommands/screenshot.rs \
        --replace-fail '"qs", "-c", "caelestia"' '"caelestia-shell"'
      substituteInPlace src/subcommands/search.rs \
        --replace-fail '"qs", "-c", "caelestia"' '"caelestia-shell"'
      substituteInPlace src/subcommands/toggle.rs \
        --replace-fail '"discord"' '"${discordBin}"' \
        --replace-fail '["todoist"]' '["todoist.desktop"]'

      substituteInPlace python-ref/src/caelestia/data/templates/qtengine.json \
        --replace-fail 'Darkly' '${qtctStyle}'
    '';

    postInstall = ''
      mkdir -p $out/share/caelestia/python
      cp -r python-ref/src/caelestia $out/share/caelestia/python/

      # Build-time replacement for the old pythonImportsCheck: fail the
      # build if the shipped python-ref tree cannot even be imported.
      ${pythonEnv}/bin/python3 -B -c "import sys; sys.path.insert(0, '$out/share/caelestia/python'); import caelestia"

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
