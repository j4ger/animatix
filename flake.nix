{
  description = "animatix devshell";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
    # Browser-automation CLI used to exercise the web/ demos against a real
    # Chromium; kept out of the default shell (start it with `nix develop .#web`).
    llm-agents.url  = "git+https://github.com/numtide/llm-agents.nix";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, llm-agents, ... }:
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
            ffmpeg
            pkg-config
            rustPlatform.bindgenHook
            alsa-lib
            pipewire  # Provides libasound_module_pcm_pipewire.so for ALSA
            nodejs
            clang

            libX11
            libXcursor
            libXrandr
            libXi
            libxcb
            libxkbcommon
            vulkan-loader
            vulkan-validation-layers
            wayland
            mesa

            rust-bin.stable.latest.default

            cocogitto
          ];

          VK_LAYER_PATH = "${pkgs.vulkan-validation-layers}/share/vulkan/explicit_layer.d";
          VK_ICD_FILENAMES = let
            icdDir = "${pkgs.mesa}/share/vulkan/icd.d";
          in
            builtins.concatStringsSep ":" (map (name: "${icdDir}/${name}") (builtins.attrNames (builtins.readDir icdDir)));

          shellHook = ''
		export LD_LIBRARY_PATH="${builtins.toString (pkgs.lib.makeLibraryPath buildInputs)}";
		export ALSA_PLUGIN_DIR="${pkgs.pipewire}/lib/alsa-lib";
          '';
        };

        # Chromium + the agent-browser CLI for driving the web/ demo pages
        # against a real browser. Deliberately not part of .#default —
        # enter with `nix develop .#web`; WebGPU needs AGENT_BROWSER_WEBGPU=1.
        devShells.web = pkgs.mkShell {
          packages = [ llm-agents.packages.${system}.agent-browser ];
        };
      }
    );
}
