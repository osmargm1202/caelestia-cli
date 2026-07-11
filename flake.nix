{
  description = "CLI for Caelestia dots";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";

    caelestia-shell = {
      url = "github:caelestia-dots/shell";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.caelestia-cli.follows = "";
    };
  };

  outputs = {
    self,
    nixpkgs,
    ...
  } @ inputs: let
    forAllSystems = fn:
      nixpkgs.lib.genAttrs nixpkgs.lib.platforms.linux (
        system: fn nixpkgs.legacyPackages.${system}
      );
  in {
    formatter = forAllSystems (pkgs: pkgs.alejandra);

    packages = forAllSystems (pkgs: rec {
      # Default: local source build. Used during development and CI; slow.
      caelestia-cli = pkgs.callPackage ./default.nix {
        rev = self.rev or self.dirtyRev;
        caelestia-shell = inputs.caelestia-shell.packages.${pkgs.system}.default;
      };

      # Prebuilt binary downloaded from a GitHub release tarball. Used by
      # downstream flakes (`~Hobby/nixos`) so `nh os switch` does not need to
      # compile Rust from source on every machine.
      caelestia-cli-bin = pkgs.callPackage ./nixos/packages/cli.nix {
        caelestia-shell = inputs.caelestia-shell.packages.${pkgs.system}.default;
        # Stable tag pin; bump via the `release.yml` workflow once per
        # upstream tag. Override at the flake-input level when bisecting.
        version = "v0.4.2";
        url = "https://github.com/osmargm1202/caelestia-cli/releases/download/v0.4.2/cli-x86_64-linux.tar.gz";
        # Checksum published in the matching release artifact
        # (cli-x86_64-linux.sha256). Auto-bumped by future workflow run.
        sha256 = "f096c9d0a23916a748a2156f7e65a454c244e9e2ed4ba8c2799a1f46fbeda1ec";
      };

      with-cli = caelestia-cli.override { withShell = true; };
      default = caelestia-cli-bin;
    });

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
  };
}
