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
          packages = [
            llm-agents.packages.${system}.agent-browser
            pkgs.static-web-server
          ];
        };

        # `nix run .#serve` — preview web/ at http://127.0.0.1:8124 with the
        # three things the player needs: application/wasm MIME, brotli
        # compression (dynamic — serves the same bytes as the prebuilt .br
        # twins), and cache headers that keep the edit-and-reload loop honest.
        # That last point is why this runs scripts/serve-web.py rather than a
        # static-web-server binary: SAS's built-in Cache-Control list (no
        # override flag exists in 2.39.0) gives .css a ONE-YEAR max-age, so a
        # returning browser renders fresh HTML against a stale cached
        # stylesheet. Serves the repo's web/ so edits are live; override with
        # SERVE_PORT and SERVE_ROOT.
        apps.serve =
          let
            script = pkgs.writeShellScript "serve-web" ''
              exec ${pkgs.python3}/bin/python3 "${./scripts/serve-web.py}" \
                "''${SERVE_PORT:-8124}" \
                "''${SERVE_ROOT:-$PWD/web}"
            '';
          in
          { type = "app"; program = toString script; };
      }
    );
}
