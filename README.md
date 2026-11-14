<p align="center">
  <img src="logo.svg" width="80" height="80" alt="Animatix">
</p>
<h1 align="center">Animatix</h1>
<p align="center"><strong>Scenes in code. Stories in motion.</strong></p>
<p align="center">A declarative language for explanatory animations, built in Rust.</p>
<p align="center">
  <a href="https://amx.j4ger.dev/demos/"><strong>See it in motion ↗</strong></a>
  &nbsp; · &nbsp;
  <a href="https://amx.j4ger.dev/tour/">Language tour</a>
  &nbsp; · &nbsp;
  <a href="docs/README.md">Documentation</a>
</p>

<p align="center">
  <a href="https://amx.j4ger.dev/demos/transformer/">
    <img src="snapshots/web/demos/transformer/scenes/overview/frame_0007_11.000s.png" width="800" alt="A Transformer architecture animated in Animatix: attention, feed-forward layers, and the next-token loop.">
  </a>
  <br>
  <sub>Made with Animatix. <a href="https://amx.j4ger.dev/demos/transformer/">Watch the Transformer walkthrough →</a></sub>
</p>

## A scene in a few lines

Describe the scene, then say what changes and when.

```animatix
config { colorscheme: "editorial-dark", resolution: (1280, 720) }

title: Text, text: "ANIMATIX", font_size: 96, color: text.primary, anchor: scene.center
accent: Rect, size: (60, 3), color: accent.primary, anchor: scene.center, offset: (0, -80)

#0.5s
fade-in title [1s, ease: ease-out]
fade-in accent [800ms]
accent.size = (640, 3) [800ms, ease: ease-out]
```

## Try it locally

**Download** · See [releases](https://github.com/j4ger/animatix/releases) for available builds. The current [Linux x86-64 nightly](https://github.com/j4ger/animatix/releases/tag/nightly) is a development snapshot, not a standalone installer; see [download caveats](docs/contributing.md#downloaded-builds). For a matching build environment, use Nix below.

<details>
<summary><strong>Build from source with Nix</strong></summary>

With [Nix](https://nixos.org/download/) and flakes enabled, open the hello scene:

```sh
git clone https://github.com/j4ger/animatix.git
cd animatix
nix develop
cargo run --bin animatix-gui -- examples/basics/00_hello.amx
```

For CLI rendering, video export, and other build setups, see the [development guide](docs/contributing.md).

</details>

---

[Contributing](CONTRIBUTING.md) · [MIT license](LICENSE)
