// <amx-player> — embed a .amx scene in any page, like <img> for animations.
//
// Usage:
//   <script type="module" src="…/amx-player.js"></script>
//   <amx-player src="./scenes/attention.amx" autoplay loop controls></amx-player>
//
// Attributes:
//   src        (required) URL of the .amx file
//   autoplay   play when scrolled into view (default unless reduced-motion/save-data)
//   loop       restart from the beginning at the end of the timeline
//   hold       seconds to keep the finished timeline on screen before a looping
//              restart (default 0.7; 0 loops with no rest at all)
//   controls   a full-height timeline strip BELOW the picture (never covers
//              it): the engine's `#` keyframe marks are drawn as ticks and
//              snapping targets (compositions add scene diamonds and hatched
//              transition spans), plus a time chip and a speed cycle
//              (1x -> 1.5x -> 2x -> 0.5x). Mouse: hovering freezes the clock
//              and peeks the frame under the pointer; leaving resumes unless
//              a click latched the pause; canvas click pauses latched, strip
//              click resumes. Touch: tap toggles, drag scrubs. Keyboard:
//              arrows step between landmarks, Home restarts, Space/K
//              toggles. With the debug readout on, Shift+arrows step exactly
//              one frame (pausing first) and `c` copies the readout.
//   title      accessibility label; shown on the skeleton while loading
//   aspect     "16:9" | "4:3" | "1:1" | "9:16" — reserve space before first frame
//              (auto-detected from the scene afterwards)
//   profile    "slim" (default) | "full" — which engine build backs this
//              element. Slim plays plain-text scenes; full adds Typst markup,
//              Code/Math highlighting and Image/Svg assets. Embeds sharing a
//              profile share one engine download and device; a page mixing
//              both downloads each once.
//   quality    "draft" (default) | "preview" | "production" — build fidelity.
//              Production matches a desktop export; changing it rebuilds.
//   sealed     page-driven mode: no center play affordance and no click
//              gestures — playback belongs to the page's own JS (play/pause/
//              seek below). With `autoplay`, the scene also resumes by itself
//              whenever it re-enters the viewport (a sealed embed has no
//              visible control to resume it with). Used for full-bleed hero
//              plates, scroll-scrubbed figures and hover-play cards.
//   debug-frame  show a frame readout in the timeline strip: the frame index
//              on a nominal 60 fps reporting grid, the exact time in seconds
//              (what `animatix image --time` takes), and the active render
//              scale. For locating and reporting a broken frame. Needs
//              `controls` (the readout lives in the strip); `?amxdebug` on the
//              page URL turns it on for every player without editing markup,
//              and `d` toggles it while the strip has focus.
//   fit        "contain" (default — the whole frame, letterboxed) | "cover"
//              — the frame fills the element box, cropping overflow. Cover
//              also unlocks the stage from its aspect-ratio box so the
//              picture fills whatever box the page gives the element (the
//              full-bleed hero). Design cover scenes with a safe center.
//   presentation  presentation/slideshow mode: automatically pauses at each
//              `#step` or `#pause` marker. Clicking the canvas, pressing Space /
//              Right Arrow / PageDown / Enter or using a presenter clicker
//              resumes playback towards the next step (or snaps immediately
//              to it if in-flight). Left Arrow / PageUp rewinds to the
//              previous step; 'f' toggles fullscreen, 'b' toggles blackout.
//
// The engine directories come from the loader <script>'s `data-runtime-base`
// (absolute, or relative to the page). A value ending in `pkg`/`pkg-slim` is
// the legacy exact-directory form (its ±slim sibling completes the pair);
// anything else is a parent directory containing both. Default: the parent of
// the directory holding this component — so an omitted attribute resolves to
// `pkg-slim`, falling back to `pkg` when that profile was not built.
//
// Loading UX (no build-time poster required):
//   1. skeleton with shimmer + title, correct aspect ratio
//   2. lazy: nothing is fetched until the element approaches the viewport
//   3. first rendered frame becomes the poster; autoplay decision follows
//   4. paused instances show a subtle play affordance
//
// Looping: a scene's timeline ends at its last keyframe — or at a declared
// `config { duration: N }`, which overrides the inferred extent — so a naive
// wrap cuts from the finished composition straight to an empty first frame.
// Looping embeds therefore run a cycle of `hold` + `duration`: the finished
// frame rests, then dissolves out and back in. Playback length comes from the
// engine's playback duration (`Timeline::playback_duration_seconds`), so a
// scene can buy itself a trailing rest by declaring a longer duration.
//
// Performance: all visible playing instances are driven by ONE shared
// requestAnimationFrame loop; offscreen instances pause automatically. Embeds
// using the same engine directory share one WebGPU context inside that wasm
// instance; a page mixing `profile` values holds one context per profile.

// Nominal frame rate for the debug readout. The player renders on rAF, so this
// is a reporting grid rather than a clock: `frame = round(t * FRAME_RATE)`. The
// seconds beside it are the reproducible value — they go straight into
// `animatix image --time` — and the grid exists so two people can name the same
// frame out loud.

import { AmxPlayerPlayback } from "./player/playback.js";

class AmxPlayerElement extends AmxPlayerPlayback {}

if (!customElements.get("amx-player")) {
  customElements.define("amx-player", AmxPlayerElement);
}

export { AmxPlayerElement };
