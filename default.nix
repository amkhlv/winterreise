with import <nixpkgs> { };

rustPlatform.buildRustPackage rec {
  name = "winterreise";
  version = "1.0";
  src = builtins.path { path = ./.; name = "winterreise"; };

  cargoHash = "sha256-Uv53DEySoL5vaUBYIinwcR1A3KXHGRGk/fJ1mM9h6yQ=";
  meta = with stdenv.lib; {
    description = "windows interactions";
  };
  nativeBuildInputs = [ pkg-config python3 ];
  buildInputs = [ pkgconfig cairo pango gdk-pixbuf gtk3 glibc glib xorg.libxcb python3 xorg.libXmu xorg.xcbutilwm ];
}
