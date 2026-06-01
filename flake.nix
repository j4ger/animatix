{
  description = "animatix devshell";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      {
        devShells.default = with pkgs; mkShell rec {
          buildInputs = [
            ffmpeg_7
            pkg-config
            rustPlatform.bindgenHook
            tree-sitter
            nodejs
            clang

	    libX11
	    libXcursor
	    libXrandr
	    libXi
	    libxcb
	    libxkbcommon
	    vulkan-loader
	    wayland

            rust-bin.stable.latest.default

            cocogitto
          ];

          shellHook = ''
		export LD_LIBRARY_PATH="${builtins.toString (pkgs.lib.makeLibraryPath buildInputs)}:/usr/lib:/usr/lib64";
          '';
        };
      }
    );
}
