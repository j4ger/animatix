# Acceptance for fragile claims

## Lock the expectation before rendering
Record only checks that could change your decision. Use the approved card or independent model, not the draft or its handoff.

- **Discrete change:** record event ID, model trigger, exact readout location, and what the last old and first new delivered frames should show. Check the cue, readout, and unchanged participant separately. Verify the next frame if a stale state could appear.
- **State that must hold:** endpoints don't prove the interval. Check the risky interior; for a short critical interval, sweep every delivered frame. Check an actual overlapping or closest-pass pose for continuous motion. Verify plotted quantities against the painted scale as well as labels.
- **Viewer inference:** give a fresh viewer only the rendered clip or ordered frames and a neutral question. Keep this plan, the brief, and the scene cards with the author. Record their answer before comparing it with the expectation. If the image is responsible for the inference, hide only external captions/narration for this diagnostic; keep on-diagram labels. A reader unable to answer isn't evidence of a different state. Recheck normal delivery.

## Capture frame evidence
For an ordinary move, render directly. For a boundary test, use the delivery frame rate and the time of the event. In this constant-rate example, frame zero starts at 0 s and the new state starts at 2 s. At 30000/1001 fps, the first frame starting at or after the cue is frame 60 (round 2 × fps up). Inspect frame 59 for the old state and frames 60–62 for the new state and its hold. For an encoded delivery with variable frame durations, use its actual frame timestamps.

```json
{
  "fps": "30000/1001",
  "checks": [
    {"frames": "59", "expect": "Clock gray; register reads 0; Ready mint"},
    {"frames": "60-62", "expect": "Clock mint; register reads 17; Ready mint"}
  ]
}
```

Use zero-based frame numbers. From this skill directory, run:

```text
python3 scripts/frame_evidence.py plan expected.json
python3 scripts/frame_evidence.py capture expected.json scene.amx proof-original --runner /path/to/render
python3 scripts/frame_evidence.py verify scene.amx proof-original
```

`plan` uses the Python 3 standard library. `capture` and `verify` also need Pillow in the same Python environment (`python3 -m pip install Pillow`). Both check that each PNG decodes. Open the images to judge their content.

For `capture`, supply a renderer adapter that accepts `render SOURCE OUT SECONDS_CSV`, writes sequential `f000.png`, `f001.png`, … at the requested times, and saves `{"times":[...],"result":{"ok":true}}` in `samples.json`. Use a new directory for each capture. Pass each imported module or asset as a repeated `--input` path relative to the scene. The receipt covers the locked plan, scene, listed inputs, runner report and saved images.

Without an adapter, sample directly with `animatix image scene.amx --time SECONDS -o frame-NNNNNN.png`. For a claim about an encoded frame boundary, inspect the decoded export at the delivery rate. Report which evidence was available.

## Decide and keep the last passing revision
Open the actual frames at delivery size and record observed states. A content mismatch at the right time calls for a content fix; change timing only when the evidence proves a timing mismatch. For continuous time, confirm required subframe thresholds separately from delivered samples. Render the same witnesses into a new folder after the edit. Recheck after the last visual edit and retain the earlier passing scene if a later change breaks a verified claim.
