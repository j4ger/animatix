# Night Reel — keynote card reel

## Goal

A 10-second dark-mode keynote intro reel: a cinematic grainy backdrop, a data
card that flies in with motion blur, a sharpening "focus pull" onto the headline,
and a levels-driven day→night grade on the closing panel. The audience is a
product-launch livestream.

## Scenes

Single scene, one continuous timeline:

1. **Backdrop** (0–10s) — checker panel inside a `Filter` with `Vignette`
   (always on, subtle) and `Grain` whose amount animates up for the intro and
   settles down; `Levels` pulls the day→night grade in the second half.
2. **Card fly-in** (1.0–2.5s) — the data card moves from off-canvas left to
   center *while a `MotionBlur` stage's `length` is high*, then the blur
   collapses to zero as it lands (motion blur as an animation accent).
3. **Focus pull** (4.0–6.0s) — `Sharpen` amount rises from 0 to 2.5 on the
   headline's `Filter` scope, holds, releases: reads as a camera focus pull.

## Constraints

- Uses only built-in effects shipped in wave 1 (Sharpen, Vignette, MotionBlur,
  Grain, Levels) plus one `Blur` for context.
- All effect parameters animate through `scope.stage.param = value` — no
  keyframe tricks, no workarounds.
- 1280×720, `editorial-dark` colorscheme, shared example assets only.
