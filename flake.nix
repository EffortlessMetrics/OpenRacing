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
          # Shell essentials
          bashInteractive
          coreutils
          gnumake
          jq
          curl
          gnused
          gnugrep
          git

          # Rust toolchain / cargo ecosystem
          rustup
          cargo-audit
          cargo-deny
          cargo-hakari
          cargo-llvm-cov
          cargo-nextest
          cargo-udeps
          sccache

          # Protobuf / buf
          protobuf
          buf

          # Build dependencies
          clang
          pkg-config
          openssl

          # Python for scripts
          python3
        ] ++ linuxOnly [
          # Linux-specific: udev for HID device access
          systemd
          libudev-zero
        ];
      in {
        devShells.default = pkgs.mkShell {
          packages = basePackages;
          shellHook = ''
            export PATH="$HOME/.cargo/bin:$PATH"
            export RUST_BACKTRACE=1
            echo "OpenRacing nix dev shell active"
            echo "Rust: $(rustc --version 2>/dev/null || echo 'not installed - run rustup show')"
            echo "Cargo: $(cargo --version 2>/dev/null || echo 'not installed')"
          '';
        };
      }
    );
}
