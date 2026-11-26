# Acceptance for fragile claims

## Lock the expectation before rendering
Record only checks that could change your decision. Use the approved card or independent model, not the draft or its handoff.

- **Discrete change:** record event ID, model trigger, exact readout location, and what the last old and first new delivered frames should show. Check the cue, readout, and unchanged participant separately. Verify the next frame if a stale state could appear.
- **State that must hold:** endpoints don't prove the interval. Check the risky interior; for a short critical interval, sweep every delivered frame. Check an actual overlapping or closest-pass pose for continuous motion. Verify plotted quantities against the painted scale as well as labels.
- **Viewer inference:** give a fresh reader one question without supplying the answer. If the image is responsible for the inference, hide only external captions/narration for this diagnostic; keep on-diagram labels. A reader unable to answer isn't evidence of a different state. Recheck normal delivery.

## Capture frame evidence
For an ordinary move, render directly; no ledger is required. For exact boundaries or windows, save a small JSON plan, e.g.:

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

Python 3 only. `plan` gives fractional seconds and the numerical samples for any renderer, with no rendering dependency. Optional `capture` uses an existing local runner, called as `render SOURCE OUT SECONDS_CSV`. It must render the requested times, write sequential `f000.png`, `f001.png`, …, and report `{"times":[...],"result":{"ok":true}}` in `samples.json`. This is an adapter contract, not an Animatix CLI command. Without it, use the project's verified renderer at the planned times. If the installed Animatix CLI supports it, `animatix image scene.amx --time SECONDS -o frame-NNNNNN.png` is a single timestamp sample. Check engine revision, timing precision, and actual output before treating a timestamp sample as an exact delivery boundary. If rendering is unavailable, mark it unverified. For captures with imports or assets, pass all inputs as repeated `--input` paths relative to the scene. The receipt checks the locked plan, scene, listed dependencies and PNGs; it doesn't infer painted states or attest to unlisted inputs or encoded playback.

## Decide and keep the last passing revision
Open the actual frames at delivery size and record observed states. A content mismatch at the right time calls for a content fix; change timing only when the evidence proves a timing mismatch. For continuous time, confirm required subframe thresholds separately from delivered samples. Render the same witnesses into a new folder after the edit. Recheck after the last visual edit and retain the earlier passing scene if a later change breaks a verified claim.
