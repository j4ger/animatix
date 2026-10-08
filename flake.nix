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
            # scripts/perf-bench.sh computes its regression verdict in Python;
            # without it the harness printed nothing and exited 0 — a green perf
            # gate that had compared nothing. It now refuses to run, so the shell
            # has to be able to run it.
            python3
            clang
            imagemagick

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

            # The release act. Pinned in practice by flake.lock's nixpkgs revision,
            # which is why CI runs `nix develop --command cog check` instead of the
            # `install.sh | sh` from `main` it used to — that downloaded whatever
            # cog tip was that day, so the tool that writes the tag and the
            # changelog was not the same tool the developer ran. `nix flake update`
            # is the act that moves cog's version.
            cocogitto
            cargo-audit
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

        # `nix develop .#web-build` — the shell that can build the wasm engine.
        # It has to be its own shell: .#default carries a rust toolchain built
        # without the wasm32 std, so `scripts/build-web.sh` dies in the first
        # dependency it touches ("can't find crate for `std`", plus the
        # misleading "target may not be installed" — the target *is* installed
        # under ~/.rustup, but this toolchain cannot see it) while also paying
        # for ALSA/FFmpeg/X it never uses.
        #
        # binaryen and brotli are the script's optional optimise/compress
        # steps; without them it falls back to `nix shell` fetches per run.
        #
        # wasm-bindgen-cli is deliberately NOT here. nixpkgs ships 0.2.114,
        # crates/animatix-web pins `=0.2.128` (wasm-bindgen-futures 0.4.78
        # releases in lockstep with it), and a mismatch fails at glue
        # generation. build-web.sh checks the version on PATH and says exactly
        # what to install rather than half-working.
        #
        # GOTCHA: devShells do not replace PATH, they prepend to it. Entering
        # this shell from inside .#default (or a direnv-loaded one) keeps the
        # OUTER rustc first on PATH, so `rustc --print sysroot` still reports
        # the toolchain without the wasm32 std and the build fails exactly as
        # before — while wasm-opt and brotli resolve correctly, which makes it
        # look like the shell loaded fine. Check with
        # `ls $(rustc --print sysroot)/lib/rustlib/`; it must list
        # wasm32-unknown-unknown. From a login shell, or with `env -i`, it does.
        devShells.web-build = pkgs.mkShell {
          packages = [
            (pkgs.rust-bin.stable.latest.default.override {
              targets = [ "wasm32-unknown-unknown" ];
            })
            pkgs.binaryen
            pkgs.brotli
            pkgs.pkg-config
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
