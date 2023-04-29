{

  nixConfig = {
    extra-substituters = [
      "https://tweag-jupyter.cachix.org"
    ];
    extra-trusted-public-keys = [
      "tweag-jupyter.cachix.org-1:UtNH4Zs6hVUFpFBTLaA4ejYavPo5EFFqgd7G7FxGW9g="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    jupyenv.url = "github:tweag/jupyenv";
  };

  outputs =
    { self, nixpkgs, rust-overlay, flake-utils, jupyenv }:
    flake-utils.lib.eachDefaultSystem (system:
    let
      overlays = [ (import rust-overlay) ];
      pkgs = import nixpkgs { inherit system overlays; };

      jupyterlab = jupyenv.lib.${system}.mkJupyterlabNew
        ({ ... }: {
          nixpkgs = nixpkgs;
          kernel.python.analysis = {
            projectDir = ./analysis;
            python = "python311";
            enable = true;
            preferWheels = true;
            # overrides = ./overrides.nix;
          };
        });
      libraries = with pkgs; [
        duckdb
      ];

      packages = with pkgs; [
        pkg-config
        openssl_3
        minisat
        cadical
        mold
        duckdb
        (rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" ];
          targets = [ "wasm32-unknown-unknown" ];
        })
      ];
    in
    {
      apps = rec {
        default = lab;
        lab = {
          program = "${jupyterlab}/bin/jupyter-lab";
          type = "app";
        };
      };
      devShells.default = pkgs.mkShell {
        buildInputs = packages;

        shellHook = ''
          export LD_LIBRARY_PATH=${
            pkgs.lib.makeLibraryPath libraries
          }:$LD_LIBRARY_PATH
        '';
      };
    });
}
