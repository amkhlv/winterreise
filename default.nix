{ rustPlatform, stdenv, pkgs ? import <nixpkgs> { } }:

rustPlatform.buildRustPackage rec {
  name = "winterreise";
  version = "1.0";
  src = builtins.path { path = ./.; name = "winterreise"; };

  cargoHash = "sha256-Uv53DEySoL5vaUBYIinwcR1A3KXHGRGk/fJ1mM9h6yQ=";
  meta = with stdenv.lib; {
    description = "windows interactions";
  };
  nativeBuildInputs = [ pkgs.pkg-config pkgs.python3 pkgs.gcc ];
  buildInputs = [ pkgs.pkg-config pkgs.gcc pkgs.cairo pkgs.pango pkgs.gdk-pixbuf pkgs.gtk3 pkgs.glibc pkgs.glib pkgs.xorg.libxcb pkgs.python3 pkgs.xorg.libXmu pkgs.xorg.xcbutilwm ];
}
