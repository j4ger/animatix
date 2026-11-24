# Audition and retake chapter narration

## Record in the native workbench

In Write, choose **Only script**, then open **Voiceover…** for a paragraph. Press **Record** and
**Stop**. The workbench keeps the original audio, aligns it locally, and
updates the scene in a single undoable edit. **More** contains audio import,
voice generation, and speech alignment settings.

For microphone alignment, use a Python 3 environment with NumPy and
`sherpa-onnx`, and make FFmpeg available to the workbench. In **More → Speech
alignment settings…**, select that Python executable, a local SenseVoice ONNX
model, and its token file. The corresponding environment variables are
`ANIMATIX_NARRATION_PYTHON`, `ANIMATIX_NARRATION_ASR_MODEL`, and
`ANIMATIX_NARRATION_ASR_TOKENS`. If alignment needs attention, the recording
stays available under **History**. **Set up speech alignment** or **Retry
alignment** stays above History for the selected paragraph. For imported audio,
the workbench can also read an existing word-timed file such as
`clip.speech.json` beside `clip.wav`.

The workbench keeps recordings beside the source in a same-named `.amxn`
file. Expand a paragraph's history to hear earlier reads and see the script as
it was recorded. Recordings are saved as they are captured, so completed audio
blocks can be recovered after an interruption. Use the normal scene save to
keep changes to the active voiceover. **Save As** copies the recording history
and brings along the current generated narration when saving to another folder.

Generated audio and its matching `.speech.json` are under `narration/` beside
the source, with the scene name and a content suffix in both filenames. The
transcript carries subtitle intervals, word timings, and cues for the scene's
animations. New automatic subtitles are centered horizontally at 85% of the
logical canvas height. Existing caption positions are kept.

If you edit a recorded paragraph, Write and Review show when the previous
voiceover is still in use. Export lists affected paragraphs before starting.
Choose **Export Draft** to continue with the current scene and its existing
assets. Still images warn when the selected frame includes previous subtitles.
Unresolved narration timing bindings must be fixed before exporting.

To share a runnable scene, copy the `.amx` and the assets it
references, including both narration files. Include the `.amxn` to share the
editing history. It can be copied as a single file after closing the editor.

## Command-line chapter tools

Use `tools/quality/speech_chapter.py` to compare voices and speaking rates for a
single sentence, then assemble chapter audio and a word-timed transcript for
Animatix. The default backend uses Edge TTS through `uv` and needs network access
for new takes. Install FFmpeg to decode Edge audio.

## Write and build a chapter

Give each sentence an ID so the tool can reuse its take when other sentences
change. Set a default voice and rate for the chapter, then override them on
individual sentences as needed. `pause_after` optionally adds silence in
seconds after the sentence audio.

```json
{
  "voice": "en-US-AndrewMultilingualNeural",
  "rate": "+0%",
  "sentences": [
    {
      "id": "opening",
      "text": "Welcome to Animatix.",
      "pause_after": 0.2,
      "markers": { "welcome": "Welcome" }
    },
    {
      "id": "timing",
      "text": "Watch the caption follow the words.",
      "markers": { "caption": "caption", "second_the": { "word": "the", "occurrence": 2 } }
    }
  ]
}
```

A marker resolves to the start of a spoken word in its sentence. Use a string
for the first occurrence or a `word` and one-based `occurrence` to select a
repeated word. A marker name must be unique across the chapter. A build fails
if the selected word is missing from the take's timings.

```sh
uv run tools/quality/speech_chapter.py build chapter.json -o media/chapter
```

This writes `media/chapter.wav` and `media/chapter.speech.json`. Bind them to the
same `Audio` actor so captions and keyframes follow the chapter.

```animatix
voice: Audio, source: "media/chapter.wav", transcript: "media/chapter.speech.json"
captions: Captions, source: "voice", caption_mode: "karaoke"

#voice@caption
  fade_in(target: captions, duration: 0.1s)
```

Keep `media/chapter.takes/` beside the generated files for subsequent builds
and retakes. The tool verifies cached audio and word timings before reusing a
take. A regular build can regenerate missing takes and reorder sentences. If
a later sentence fails, the next build can reuse earlier validated takes.

## Correct an external transcript and export subtitles

Point a `Captions` actor at Whisper JSON, speech JSON, SRT, or VTT. The CLI
can normalize external recognition output and apply corrections before you
build the scene.

```bash
animatix transcript whisper.json --edits edits.json -o voice.speech.json
animatix subtitles scene.amx --track captions -o voice.vtt
```

The optional edits JSON is a list of edits keyed by segment `id`. Each entry
can replace `text`, `start_s`, and `end_s`. A `words` list selects words by
zero-based `index` and can replace their time bounds. When changing a word's
spelling, supply the corrected segment text as well.

```json
[{"id": 7, "text": "The result", "words": [{"index": 1, "word": "result", "start_s": 1.4}]}]
```

The `.srt` or `.vtt` suffix selects the export format. Export reads logical
cues on the scene or composition clock, including audio offsets, trims,
and scene offsets. `--track` selects a Captions actor by label. On-screen
captions can additionally use `caption_pause` in seconds, `caption_outline` and
`caption_outline_width`, `caption_shadow` and `caption_shadow_color`, or
`word_backdrop_color` for the active word.

## Choose a read and retake one sentence

Pass a sentence ID and repeat `--candidate` with `VOICE@RATE` or a voice alone
to use that sentence's current rate. Choose a new output directory for each
audition. Play `comparison.wav` to hear the candidates in command-line order,
with a half-second gap between reads. The numbered candidate WAVs support
individual playback.

```sh
uv run tools/quality/speech_chapter.py audition chapter.json opening -o reads/opening \
  --candidate en-US-AndrewMultilingualNeural@+0% \
  --candidate en-US-AvaMultilingualNeural@-12%
```

To use the second candidate, add `"voice": "en-US-AvaMultilingualNeural"` and
`"rate": "-12%"` to the `opening` sentence, then apply the exact recorded take.

```sh
uv run tools/quality/speech_chapter.py retake chapter.json opening -o media/chapter \
  --from-audition reads/opening --candidate 2
```

To synthesize a fresh take after an edit, omit `--from-audition` and
`--candidate`. Retake requires the last build's sentence order and reusable
takes for the other sentences. It updates the word and caption timings and
reports each affected named marker's previous time, new time, and delta in
seconds. Added or removed markers have a missing time shown as `null`.

## Use Qwen3-TTS locally for Mandarin

The optional `qwen_chapter_provider.py` adapter runs Qwen3-TTS CustomVoice
for stock speakers such as `Uncle_Fu` and `Serena`. It uses SenseVoice through
sherpa-onnx to recognize the rendered Mandarin audio with token timestamps.
FFmpeg applies the requested speed before recognition, so the timed transcript
follows the WAV at that speed.

Use a separate Python environment with matching CPU PyTorch and torchaudio
builds, `qwen-tts` 0.1.1, and `sherpa-onnx` 2.0.1. Activate it before invoking
the provider. Put FFmpeg on `PATH`. The adapter loads a local Qwen3-TTS
CustomVoice directory with its speech tokenizer, a local SenseVoice ONNX
model, and its `tokens.txt`. It uses local files for inference.

For example, save the following as `mandarin.json`.

```json
{
  "voice": "Uncle_Fu",
  "sentences": [
    { "id": "opening", "text": "你看见了一只猫。", "markers": { "cat": "猫" } },
    { "id": "next", "text": "左边是一张照片。", "markers": { "photo": "照片" } }
  ]
}
```

Save `{"照片": []}` as `lexicon.json` to group `照片` into a single timed word.
An entry such as `"它": ["他"]` also allows an explicitly reviewed recognition
spelling and logs each use. The adapter otherwise compares the complete
authored sentence to recognized tokens after removing punctuation and spaces
and normalizing character width and case. Unmatched, extra, and ambiguous
tokens or unusable timestamps stop the take. Han characters without a
lexicon entry are timed individually. Timing precision depends on SenseVoice's
token onsets. Word ends use the next onset or the end of the take. The adapter
omits confidence scores.

Run from the repository root, replacing the `/models/` paths with your model
paths. Keep the same provider arguments for the three commands.

```sh
provider=(
  --provider-command "$(command -v python3)"
  --provider-arg "$PWD/tools/quality/qwen_chapter_provider.py"
  --provider-arg=--tts-model --provider-arg /models/Qwen3-TTS-12Hz-0.6B-CustomVoice
  --provider-arg=--asr-model --provider-arg /models/SenseVoice/model.int8.onnx
  --provider-arg=--asr-tokens --provider-arg /models/SenseVoice/tokens.txt
  --provider-arg=--lexicon --provider-arg "$PWD/lexicon.json"
  --provider-revision qwen-zh-v1
)
python3 tools/quality/speech_chapter.py build mandarin.json -o media/mandarin "${provider[@]}"
python3 tools/quality/speech_chapter.py audition mandarin.json opening -o reads/mandarin \
  --candidate Serena@+0% --candidate Uncle_Fu@-12% "${provider[@]}"
```

To choose the second read, set `"rate": "-12%"` on the `opening` sentence.

```sh
python3 tools/quality/speech_chapter.py retake mandarin.json opening -o media/mandarin \
  --from-audition reads/mandarin --candidate 2 "${provider[@]}"
```

Change `--provider-revision` after replacing model files or editing the
lexicon at the same path, then build to refresh the cached takes. A fresh take
can be retried if the speech or recognition differs from the script.

## Use another local voice provider

Pass `--provider-command` with the executable that runs your local model and
word aligner. Add a `--provider-arg` for each argument.

The adapter runs from the chapter JSON's directory. It reads one JSON object
from standard input with `text`, `voice`, `rate`, and two absolute output paths.
Use your local voice identifiers in the chapter and interpret `rate` as a
relative speed change.

- At `audio`, write a WAV with the rendered sentence. Mono 24 kHz, 16-bit PCM
  works with Python alone. Other WAV formats need FFmpeg for conversion.
- At `words`, write a JSON array of word boundaries on the audio's time axis,
  such as `[{"word":"Welcome","start":0.1,"end":0.48}]`. Include trailing
  punctuation where appropriate. `confidence` is optional.

Supply the same provider arguments for build, audition, and retake. Change
`--provider-revision` when the model, adapter, or voice reference changes so
the next build refreshes affected audio and alignment.
