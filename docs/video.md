# Video clips

Use a `Video` actor to place MP4 or MOV footage behind AMX text, diagrams or
annotations. The native workbench, CLI and Web player share the same scene
timing, trim and layout rules. Native builds use the `video` feature to
decode footage with rsmpeg and linked FFmpeg 8 libraries, including when
rendering a PNG or storyboard. Video export uses the same libraries.

## Put a recording on the timeline

Copy a screen recording to `recording.mp4`. Use a recording at least five
seconds long, or adjust the trim to fit the source. Save this scene beside it
as `demo.amx`.

```animatix
config { canvas_size: (1080, 1920), duration: 6 }

#0s
  title: Text, text: "Find the export settings", at: (540, 320), font_size: 64

#+750ms
  demo: Video, source: "recording.mp4", in_point: 1s, out_point: 5s,
    at: (540, 920), size: (900, 720), fit: "contain",
    end_behavior: "hold", volume: 0.2
  pointer: Arrow, from: (230, 1120), to: (460, 980), stroke: accent.primary, stroke_width: 10

#+1s
  demo.volume = 0 [300ms]
```

The clip plays from `0.75s` to `4.75s` on the scene clock. The final picture
holds until the scene ends at `6s`. To schedule it from a transcript marker,
replace `#+750ms` with a time header such as `#voice@show_settings + 250ms` and
declare the corresponding narration track.

```sh
animatix check demo.amx
animatix storyboard demo.amx
animatix image demo.amx --time 2 -o frame.png
animatix video demo.amx --fps 30 --hold 0 -o demo.mp4
```

## Source time and output

- The source range uses seconds or milliseconds on the file's common media
  clock. At 1×, each elapsed scene second advances the clip by one source
  second. The frame's presentation timestamp determines when it appears,
  including in a variable-frame-rate recording. Output frame rate controls
  how often the scene is sampled.
- The renderer applies display rotation and pixel aspect ratio before
  positioning the crop. `crop` is normalized in the oriented source.
  `fit` maps the crop to the actor viewport. Any space around a contained
  clip stays transparent.
- The first video stream is used, excluding cover art. The first audio
  stream is mixed with other audio in native playback and export when its
  gain is above zero. Native audio is trimmed and scheduled on the same clock
  as the picture. Video is muted on the Web.
- The native backend reads local MP4/MOV files. The Web player resolves a
  relative `source` against the AMX file that declares it and uses the browser's
  `<video>` decoder. Serve the file from the page's origin or allow anonymous
  CORS requests so the player can read the pixels. HTTP byte ranges let the
  browser seek without downloading the whole file. H.264 in MP4 is the
  cross-platform baseline. Codec support for other sources depends on the host.
- Use display sizes up to 8192 pixels per side and 16,777,216 pixels per frame.
  The Web player prepares the requested picture before rendering the scene.
  Dragging rapidly replaces pending requests with the latest playhead position.
  The browser's decoder and media server determine seek latency.

For the full property list, see [Video in the primitive reference](primitives.md#video).
