{
  description = "A voice assistant project intended to run on a Raspberry Pi";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forEachSupportedSystem = f: nixpkgs.lib.genAttrs supportedSystems (system: f {
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            rust-overlay.overlays.default
            self.overlays.rust
            self.overlays.raspberry
            (final: prev: {
              # onnxruntime = (prev.onnxruntime.overrideAttrs (finalAttrs: previousAttrs:
              #   let
              #     version = "1.20.0";
              #   in
              #   {
              #     inherit version;
              #     src = prev.fetchFromGitHub {
              #       owner = "microsoft";
              #       repo = "onnxruntime";
              #       rev = "refs/tags/v${version}";
              #       hash = "sha256-bYgoyrJxI7qMOU6k4iYwd4n+ecXPKAPvatvCBUf8VP4="; # sha256-+zWtbLKekGhwdBU3bm1u2F7rYejQ62epE+HcHj05/8A=
              #       fetchSubmodules = true;
              #     };
              #
              #     patches = [
              #       ./update-re2.patch
              #       ./0001-eigen-allow-dependency-injection.patch
              #     ];
              #   })).override { pythonSupport = false; cudaSupport = false; ncclSupport = false; };
              onnxruntime = prev.callPackage ./nix/onnxruntime-bin.nix { };
            })
          ];
        };
      });
    in
    {
      overlays = {
        rust = final: prev: {
          rustToolchain =
            let
              rust = prev.rust-bin;
            in
            if builtins.pathExists ./rust-toolchain.toml then
              rust.fromRustupToolchainFile ./rust-toolchain.toml
            else if builtins.pathExists ./rust-toolchain then
              rust.fromRustupToolchainFile ./rust-toolchain
            else
              rust.stable.latest.default.override {
                extensions = [ "rust-src" "rustfmt" ];
              };
        };

        raspberry = final: prev: {
          raspberry = self.packages.${prev.stdenv.hostPlatform.system}.default;
        };

        default = self.overlays.raspberry;
      };

      nixosModules = {
        raspberry = import ./nix/module.nix self;
      };

      devShells = forEachSupportedSystem ({ pkgs }: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            openssl
            pkg-config
            cargo-deny
            cargo-edit
            cargo-watch
            rust-analyzer
            alsa-lib
            rustPlatform.bindgenHook
            speechd
            libclang.lib
            onnxruntime
            # (pkgs.callPackage (import ./vosk-api.nix) { })
            (pkgs.callPackage (import ./nix/vosk-api-bin.nix) { })
          ];

          env =
            {
              # Required by rust-analyzer
              RUST_SRC_PATH = "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
            };
        };
      });

      packages = forEachSupportedSystem ({ pkgs }: {
        raspberry = pkgs.callPackage ./nix/default.nix { };
        default = self.packages.${pkgs.system}.raspberry;
      });
    };
}
