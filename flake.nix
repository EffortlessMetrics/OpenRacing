{
  description = "OpenRacing Nix dev shell and CI tooling";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        linuxOnly = lib.optionals pkgs.stdenv.isLinux;
        basePackages = with pkgs; [
          bashInteractive
          buf
          cargo-audit
          cargo-deny
          cargo-hakari
          cargo-llvm-cov
          cargo-udeps
          clang
          git
          openssl
          pkg-config
          protobuf
          python3
          rustup
          sccache
        ] ++ linuxOnly [
          systemd
        ];
      in {
        devShells.default = pkgs.mkShell {
          packages = basePackages;
          shellHook = ''
            export PATH="$HOME/.cargo/bin:$PATH"
            export RUST_BACKTRACE=1
          '';
        };
      }
    );
}
