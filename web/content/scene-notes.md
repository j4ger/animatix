# Scene contracts and verification

Every source is self-contained. Page titles and long explanations belong in the DOM; the canvas carries only labels needed to understand its visual argument. The Showcase works use 1280×720 stages. Each work has its own backdrop and uses low-contrast decoration behind the instructional graphics.

## Works

| Source | Duration | Build-time controls | Visual argument |
|---|---:|---|---|
| `scenes/move-the-basis.amx` | 24 s | none | A horizontal shear changes shape and direction while preserving area against a fixed reference. |
| `scenes/make-room.amx` | 24 s | none | Stable insertion sort preserves the order of equal keys while shifting a sorted prefix. |
| `scenes/one-rule-many-forms.amx` | 20 s | `preset=0` or `1` | One shape → 48 copies → distance controls delay → changing the distance rule changes the choreography. Not a benchmark. |
| `scenes/when-waves-meet.amx` | 28 s | `phase_degrees=0`, `90`, `180` | Reveal A and B, add their heights at one x position, reveal the full sum, then change the phase. |
| `scenes/words-keep-time.amx` | 20 s | none | Turn a maintenance announcement into a notice with distinct fields for impact, timing and what still works. |

### One Rule

The seed holds until 1.7 s. Array reveal starts at 2 s, with a 25 ms per-index offset. The first pulse starts at 5.5 s plus distance delay. At 10 s only the delay metric changes; pulses at 11 and 16 s demonstrate it. The final state holds at 19–20 s.

Preset 0 starts radial and switches to diagonal; preset 1 reverses that order. The formulas displayed on canvas are the formulas actually used. `Grid` owns fixed spacing while scale, rotation and opacity vary inside the slots. The Gaussian envelope explicitly uses `exp(0 - x^2)` because the DSL's unary-minus precedence made an earlier `exp(-x^2)` version overflow. The defective version was caught by actual rendering and is not delivered.

### Waves

A enters at 0.2 s, B at 2.5 s. Single-point addition appears at 5.5–8.5 s; the complete sum enters at 8.5 s. The phase panel replaces that annotation at 11 s. Initial phase holds until 12 s, moves by +180° at 12–18 s, holds until 21 s, moves to +360° at 21–26 s, then holds through 28 s.

- A(x)=sin(x); B(x)=sin(x+phase); the output is exactly A(x)+B(x).
- All plots use 40 px per amplitude unit; the output has extra vertical headroom.
- Phase, amplitude, sample-point values, curves and dial derive from the same analytic phase. Amplitude is `2*abs(cos(phase/2))`.
- Continuous numeric strings update immediately, without repeatedly starting a whole-string morph.
- Semantic labels are static glyphs with analytic phase/amplitude-dependent opacity. They fade through a brief clear gap, never superimposing two competing labels. This works for initial-phase presets and arbitrary seeking without accumulated transition state.
- Initial phase 180° starts cancelled, reinforces midway and ends cancelled. No caption hardcodes the default narrative.

### Maintenance notice

The original announcement is visible from the first frame. At 4 s it moves left and the impact appears on the right. The time window and available features follow at 8 and 12 s. The final comparison holds from 16 to 20 s. A `Stack` aligns the original card and its text, while a `Col` places three instances of `NoticeField` without separate row coordinates. The time window is authored once.

## Hero and lessons

| Source | Duration | Single purpose |
|---|---:|---|
| `scenes/hero.amx` | 7 s | Periodic three-shape brand motif, complete start/end composition |
| `scenes/first-move.amx` | 3 s | Declare a dot; animate `at` with a `#` marker |
| `scenes/compose.amx` | 4 s | Group transformation and Row alignment |
| `scenes/repeat.amx` | 5 s | Generated array in a Grid |
| `scenes/values.amx` | 6 s | Stateless `always` from absolute time |
| `scenes/timing.amx` | 6 s | Explicit absolute timeline markers |
| `scenes/finish.amx` | 5 s | Complete silent scene with a held poster frame |

Lessons are intentionally minimal, not duplicated work scenes. Lesson 06 can demonstrate `animatix image finish.amx --time 5 -o poster.png`. Video requires a CLI built with the `video` feature.

## Live playback and recorded fallback

The 15 current works, lessons and techniques use their published AMX sources.
Choose **Play live** on the poster to start the engine. The poster remains
visible during load or after an error. Showcase works also offer **Play
recording** after a live playback failure. Recordings always show the authored
default settings.

`preview-manifest.json` binds each work to its source, poster and optional
recording. All five recordings are silent H.264/yuv420p, 1280×720 at 24 fps,
with MP4 fast-start and verified frame counts. The manifest carries each
duration, poster timestamp and artifact hash.

Render at the authored resolution: the current CLI's smaller `--width/--height` video options cropped this scene instead of scaling. The initial low-resolution attempt was rejected after viewing encoded frames. Current delivered recordings have no crop or resize.

## Acceptance record

The unchanged hero and six lessons previously passed native CLI check and actually viewed storyboards. Hero 0/7 s full-resolution PNGs were pixel-identical. Lesson samples: first-move 0/0.5/1.25/2/3; compose 0/1/1.5/3/4; repeat 0/1/2/3/5; values 0/1.5/3/4.5/6; timing 0/1.5/2.5/3.5/6; finish 0/1/1.5/2.5/5.

The three refined works passed native CLI check, including One Rule preset 1 and Waves initial phases 90°/180° (unused-label hints only). The final Waves annotation renders negative additions with parentheses.

Actual rendered samples inspected:
- One Rule: default recording sampled at 1 fps; preset 1 storyboard at 0/3/6.5/12/17/20 s. Both switch the actual delay metric, then settle.
- Waves: phases 0°/90°/180°, each at 7/13.5/17.5/19/21.5/28 s. Curves, phase, resultant amplitude and semantic state agree. An earlier crossfade that superimposed labels was rejected and replaced by fade-through.
- The earlier Words study was reviewed at 0/3/7/11/15.5/20 s. Maintenance notice replaces that study at the same URL.

All three complete recordings were exported and their encoded frames sampled at 1 fps for visual inspection. ffprobe confirms 1280×720, 24 fps, exact 480/672/480 frames and 20/28/20 s. Posters are native full-resolution frames at 6.5/15/20 s; manifest hashes identify the final sources, posters and recordings. This is complete export plus sampled visual review, not a claim of watching every frame in real time. Native QA does not establish browser WebGPU playback or audio acceptance.

## Voiceover remains separate

The repository has a narration example, word-level JSON, an Edge-TTS helper, and `Audio`/speech-anchor/`Captions` support. This investigation did not establish asset-specific provenance for that existing voice, and its script does not match Words. A voiced edition needs an approved script/voice and provenance, actual audio plus word timings, speech-anchor choreography, listening/sync review, and browser audio scheduling/autoplay/seek acceptance. A synthesized metronome is not a substitute for narration.

## Completion-pass stories

- `move-the-basis.amx` (24 s): a horizontal shear with one shared amount and a
  fixed grey reference. The unit area is preserved; lengths and angles change.
  No public parameter is advertised: the authored tour deliberately compares
  zero, positive and negative shear on one fixed scale.
- `make-room.amx` (24 s): stable insertion sort on `[4, 2a, 5, 2b, 3]`. Counters
  count eight key comparisons and five rightward moves, excluding key placement.
  Explicit keyframe events expose each decision. `NumberCard` moves the face and digit together, slot positions and prefix widths share one spacing value, and a timeline function schedules subtitle transitions from their authored cue times.
- `technique-reveal/focus/stagger/labels.amx`: four short, single-intent sources
  linked from Learn. Unlike works, techniques do not need a separate story arc.

See `docs/web-reset-completion.md` for editorial scope and the final review
report for the actual rendering/build coverage of this pass.
