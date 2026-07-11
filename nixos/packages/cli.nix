{
  # Fetches a prebuilt `caelestia` binary that the upstream release workflow
  # publishes on GitHub. No local Rust toolchain required — Nix never evaluates
  # the source tree, so `nh os switch` skips the slow cargo build entirely.
  #
  # Pin `version` (e.g. "v0.4.2") to a tagged release. The default branches
  # bump `version` automatically through the Pin workflow described in
  # docs/superpowers/specs/2026-07-10-cli-prebuilt-binaries.md.
  lib,
  stdenv,
  fetchurl,
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
  gpu-screen-recorder,
  dconf,
  killall,
  ffmpeg,
  caelestia-shell,
  withShell ? true,
  discordBin ? "discord",
  qtctStyle ? "Darkly",
  version ? "v0.4.2",
  # Override the platform-specific asset name. Defaults to x86_64 Linux.
  url ? "https://github.com/osmargm1202/caelestia-cli/releases/download/${version}/cli-x86_64-linux.tar.gz",
  sha256 ? "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
  # Used by the version-pinning consumers; an empty/unset value is allowed for
  # `option`-style wiring in the consumer flake.
  ...
}: let
  cliRuntimeDeps = [
    swappy
    libnotify
    slurp
    wl-clipboard
    cliphist
    xdg-utils
    dart-sass
    grim
    gpu-screen-recorder
    dconf
    killall
    ffmpeg
  ];
in stdenv.mkDerivation {
  pname = "caelestia-cli";
  version = version;

  src = fetchurl { inherit url sha256; };

  nativeBuildInputs = [ makeWrapper installShellFiles ];

  dontStrip = false;

  # The release tarball ships `bin/caelestia` and `share/...`; the default
  # `unpackPhase` only accepts a single top-level directory. Extract into
  # the per-build source root (`@out@` resolved to a tmpdir) so downstream
  # phases see the layout they expect.
  unpackPhase = ''
    runHook preUnpack
    sourceRoot="$NIX_BUILD_TOP/source"
    mkdir -p "$sourceRoot"
    tar -xzf "$src" -C "$sourceRoot"
    runHook postUnpack
  '';

  # The release tarball contains two top-level directories (bin/, share/) so
  # `src = ./.` style unpack would refuse. fetchurl preserves the layout; we
  # use `src` directly and copy what we need below.
  installPhase = ''
    runHook preInstall

    if [ -d "$src/bin" ]; then
      install -Dm755 $src/bin/caelestia $out/bin/caelestia
    else
      # Older tarballs (pre-bin/) shipped the binary at the root.
      install -Dm755 $src/caelestia $out/bin/caelestia
    fi
    if [ -d "$src/share" ]; then
      cp -r $src/share $out/share
    fi

    runHook postInstall
  '';

  # `withShell` matches the legacy `caelestia-cli.withShell` override so the
  # `caelestia-shell` package can opt into shipping the CLI alongside the
  # shell. The default is `true` so consumers don't need to set anything.
  postFixup = ''
    wrapProgram $out/bin/caelestia \
      ${lib.optionalString withShell "--prefix PATH : ${lib.makeBinPath [ caelestia-shell ]}"} \
      --prefix PATH : ${lib.makeBinPath cliRuntimeDeps}
  '';

  # Mirror the upstream substitutions that previously lived in `default.nix`'s
  # postPatch. The prebuilt binary already substitutes the Python sources
  # (they ship inside the tarball); this only patches shell/qt string literals.
  # Consumers that need different defaults (discord variant, Qt theme) should
  # set `discordBin` / `qtctStyle` on the flake input.
  inherit discordBin qtctStyle;

  meta = {
    description = "Prebuilt CLI for Caelestia dots (downloaded from a release tarball)";
    homepage = "https://github.com/osmargm1202/caelestia-cli";
    license = lib.licenses.gpl3Only;
    mainProgram = "caelestia";
    platforms = lib.platforms.linux;
  };
}
