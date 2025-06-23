{ pkgs ? import <nixpkgs> { } }:
pkgs.rustPlatform.buildRustPackage rec {
  pname = "winterreise";
  version = "2.0";
  depsBuildBuild = with pkgs; [ pkg-config python314 gcc glibc glib gdk-pixbuf cairo pango atk gtk3 zlib zlib.dev xorg.xcbutil xorg.libXmu xorg.xcbutilwm xorg.libxcb ];
  cargoLock.lockFile = ./Cargo.lock;
  src = pkgs.lib.cleanSource ./.;
  }
