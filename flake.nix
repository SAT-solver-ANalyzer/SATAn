{
  nixConfig = {
    extra-substituters = [ "https://tweag-jupyter.cachix.org" ];
    extra-trusted-public-keys = [
      "tweag-jupyter.cachix.org-1:UtNH4Zs6hVUFpFBTLaA4ejYavPo5EFFqgd7G7FxGW9g="
    ];
  };

  inputs = {
    nixpkgs-unstable.url = "github:nixos/nixpkgs/nixos-unstable";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-22.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    jupyenv.url = "github:tweag/jupyenv";
  };

  outputs =
    { self, nixpkgs, rust-overlay, flake-utils, jupyenv, nixpkgs-unstable }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            rust-overlay.overlays.default
            (self: super: {
              unstable = (import nixpkgs-unstable { inherit system; });
            })
          ];
        };

        jupyterlab = jupyenv.lib.${system}.mkJupyterlabNew ({ ... }: {
          nixpkgs = nixpkgs;
          kernel.python.analysis = {
            projectDir = ./analysis;
            python = "python311";
            enable = true;
            preferWheels = true;
          };
        });
        libraries = with pkgs; [ duckdb openssl_3 sqlite ];

        packages = with pkgs;
          libraries ++ [
            pkg-config
            minisat
            cadical
            mold
            kissat
            bacon
            (rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" ];
              targets = [ "x86_64-unknown-linux-musl" ];
            })
          ];
      in {
        apps = rec {
          default = lab;
          lab = {
            program = "${jupyterlab}/bin/jupyter-lab";
            type = "app";
          };
        };

        devShells = rec {
          default = runner;
          runner = pkgs.mkShell {
            buildInputs = packages;

            shellHook = ''
              export LD_LIBRARY_PATH=${
                pkgs.lib.makeLibraryPath libraries
              }:$LD_LIBRARY_PATH
            '';
          };
          docs = pkgs.mkShell {
            buildInputs = with pkgs; [
              python311Packages.mkdocs-material
              nodePackages_latest.prettier
            ];
          };
          vm = pkgs.mkShell { buildInputs = with pkgs; [ unstable.vagrant ]; };
        };

      });
}
