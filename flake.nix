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

      with-cli = caelestia-cli.override { withShell = true; };
      default = caelestia-cli;
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
