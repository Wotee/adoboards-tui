let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11";
  pkgs = import nixpkgs {
    config = {};
    overlays = [];
  };
in
  pkgs.mkShell {
    buildInputs = [
      pkgs.cargo
      pkgs.rustc
      pkgs.rustfmt

      # Necessary for the openssl-sys crate:
      pkgs.openssl
      pkgs.pkg-config
    ];

    # See https://discourse.nixos.org/t/rust-src-not-found-and-other-misadventures-of-developing-rust-on-nixos/11570/3?u=samuela.
    RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
  }
