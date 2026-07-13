{
  rev,
  lib,
  rustPlatform,
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
}: let
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

    postPatch = ''
      substituteInPlace src/subcommands/shell.rs \
        --replace-fail '"qs", "-c", "caelestia"' '"caelestia-shell"'
      substituteInPlace src/subcommands/screenshot.rs \
        --replace-fail '"qs", "-c", "caelestia"' '"caelestia-shell"'
      substituteInPlace src/subcommands/search.rs \
        --replace-fail '"qs", "-c", "caelestia"' '"caelestia-shell"'
      substituteInPlace src/subcommands/toggle.rs \
        --replace-fail '"discord"' '"${discordBin}"' \
        --replace-fail '["todoist"]' '["todoist.desktop"]'
    '';

    postInstall = ''
      installShellCompletion completions/caelestia.fish
    '';

    postFixup = ''
      wrapProgram $out/bin/caelestia \
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
