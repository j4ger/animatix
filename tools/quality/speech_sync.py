#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = [
#     "edge-tts==7.3.1",
# ]
# ///
"""
Speech & Caption Synchronizer for Animatix.

Generates real audio (.mp3) and word-level speech transcripts (.speech.json)
for Animatix scenes, compatible with #voice@marker, #voice["word"], and
first-class `Captions` primitives.

Usage:
  # Text-to-Speech synthesis with word timestamps:
  uv run tools/quality/speech_sync.py synthesize \\
      --text "Welcome to Animatix. The layout-first animation DSL for technical storytelling." \\
      --markers "intro:0.1,feature:1.9,captions:6.2,outro:9.8" \\
      --output examples/assets/narration

  # Convert Whisper JSON or SubRip subtitles to universal .speech.json:
  uv run tools/quality/speech_sync.py convert input.json -o output.speech.json
  uv run tools/quality/speech_sync.py convert input.srt -o output.speech.json
"""

import argparse
import asyncio
import json
import re
import sys
from pathlib import Path

try:
    import edge_tts
except ImportError:
    edge_tts = None


async def collect_edge_speech(text: str, voice: str, rate: str = "+0%"):
    if edge_tts is None:
        raise ValueError("`edge-tts` is required. Run the tool with `uv run`.")

    options = {"boundary": "WordBoundary"}
    if rate != "+0%":
        options["rate"] = rate
    communicate = edge_tts.Communicate(text, voice, **options)
    words = []
    audio = bytearray()
    async for chunk in communicate.stream():
        if chunk["type"] == "audio":
            audio.extend(chunk["data"])
        elif chunk["type"] == "WordBoundary":
            # Edge reports 100 ns ticks. Keep their precision until formatting the transcript.
            words.append({
                "word": chunk["text"],
                "start": chunk["offset"] / 10_000_000,
                "end": (chunk["offset"] + chunk["duration"]) / 10_000_000,
                "confidence": 1.0,
            })
    if not audio or not words:
        raise ValueError("synthesis did not return both audio and word boundaries. No files were written.")
    return bytes(audio), words


async def synthesize_speech(
    text: str,
    output_prefix: str,
    voice: str = "en-US-AndrewMultilingualNeural",
    markers_dict: dict = None,
):
    audio_path = f"{output_prefix}.mp3"
    json_path = f"{output_prefix}.speech.json"

    try:
        audio_data, words = await collect_edge_speech(text, voice)
    except ValueError as error:
        sys.exit(f"Error: {error}")
    for word in words:
        word["start"] = round(word["start"], 3)
        word["end"] = round(word["end"], 3)

    with open(audio_path, "wb") as f:
        f.write(audio_data)

    total_duration = words[-1]["end"] + 0.3

    # Group words into sentence segments by punctuation
    segments = []
    current_words = []
    seg_id = 0

    for w in words:
        current_words.append(w)
        if any(w["word"].endswith(punct) for punct in [".", "!", "?", "。", "！", "？"]):
            seg_text = " ".join(cw["word"] for cw in current_words)
            segments.append({
                "id": seg_id,
                "start": current_words[0]["start"],
                "end": current_words[-1]["end"],
                "text": seg_text,
                "words": list(current_words),
            })
            seg_id += 1
            current_words = []

    if current_words:
        seg_text = " ".join(cw["word"] for cw in current_words)
        segments.append({
            "id": seg_id,
            "start": current_words[0]["start"],
            "end": current_words[-1]["end"],
            "text": seg_text,
            "words": list(current_words),
        })

    transcript_data = {
        "duration": round(total_duration, 3),
        "markers": markers_dict or {},
        "segments": segments,
    }

    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(transcript_data, f, indent=2, ensure_ascii=False)

    print(f"✓ Generated audio:      {audio_path} ({len(audio_data)} bytes)")
    print(f"✓ Generated transcript: {json_path} ({len(words)} words, {len(segments)} segments)")


def parse_markers(markers_str: str) -> dict:
    if not markers_str:
        return {}
    res = {}
    for part in markers_str.split(","):
        if ":" in part:
            k, v = part.split(":", 1)
            res[k.strip()] = float(v.strip())
    return res


def parse_srt_timestamp(timestamp: str) -> float:
    match = re.fullmatch(r"\s*(?:(\d+):)?(\d{2}):(\d{2})[,.](\d{3})(?:\s+.*)?", timestamp)
    if not match:
        raise ValueError(f"invalid timestamp '{timestamp.strip()}'")
    hours, minutes, seconds, milliseconds = match.groups()
    return int(hours or 0) * 3600 + int(minutes) * 60 + int(seconds) + int(milliseconds) / 1000


def parse_srt(content: str) -> dict:
    segments = []
    for index, block in enumerate(re.split(r"\n(?:[ \t]*\n)+", content.strip()), start=1):
        lines = block.splitlines()
        if lines and lines[0].strip().isdigit():
            lines = lines[1:]
        if not lines or "-->" not in lines[0]:
            raise ValueError(f"cue {index} is missing its timing line")

        start_text, end_text = lines[0].split("-->", 1)
        try:
            start = parse_srt_timestamp(start_text)
            end = parse_srt_timestamp(end_text)
        except ValueError as error:
            raise ValueError(f"cue {index}: {error}") from error

        text = " ".join(line.strip() for line in lines[1:])
        # Match the native SRT loader: each token inherits its cue's time span.
        segments.append({
            "start": start,
            "end": end,
            "text": text,
            "words": [{"word": word, "start": start, "end": end} for word in text.split()],
        })
    return {"segments": segments}


def read_number(record: dict, fields: tuple[str, ...], default: float) -> float:
    for field in fields:
        value = record.get(field)
        if value is not None:
            return float(value)
    return default


def convert_transcript(input_path: str, output_path: str):
    p = Path(input_path)
    if not p.exists():
        sys.exit(f"Error: input file '{input_path}' not found.")

    content = p.read_text(encoding="utf-8-sig")
    if p.suffix.lower() == ".srt":
        try:
            data = parse_srt(content)
        except ValueError as error:
            sys.exit(f"Error: invalid SRT in '{input_path}': {error}.")
    else:
        data = json.loads(content)

    markers = data.get("markers", {})
    segments = []

    raw_segs = data.get("segments") or data.get("cues") or []
    for i, seg in enumerate(raw_segs):
        start = read_number(seg, ("start", "start_s"), 0.0)
        end = read_number(seg, ("end", "end_s"), start)
        text = seg.get("text", "").strip()
        words = []
        for w in seg.get("words", []):
            w_start = read_number(w, ("start", "start_s"), start)
            w_end = read_number(w, ("end", "end_s"), w_start)
            words.append({
                "word": w.get("word") or w.get("text", ""),
                "start": round(w_start, 3),
                "end": round(w_end, 3),
                "confidence": read_number(w, ("confidence", "probability"), 1.0),
            })
        segments.append({
            "id": i,
            "start": round(start, 3),
            "end": round(end, 3),
            "text": text,
            "words": words,
        })

    duration = read_number(data, ("duration",), segments[-1]["end"] if segments else 0.0)

    out_data = {
        "duration": round(duration, 3),
        "markers": markers,
        "segments": segments,
    }

    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(out_data, f, indent=2, ensure_ascii=False)

    print(f"✓ Converted to universal transcript: {output_path}")


def main():
    parser = argparse.ArgumentParser(description="Animatix Speech & Captions Tool")
    subparsers = parser.add_subparsers(dest="command", required=True)

    syn_parser = subparsers.add_parser("synthesize", help="Synthesize TTS with word timestamps")
    syn_parser.add_argument("--text", "-t", required=True, help="Spoken voiceover text")
    syn_parser.add_argument(
        "--output", "-o", default="narration", help="Output prefix (writes .mp3 and .speech.json)"
    )
    syn_parser.add_argument(
        "--voice", "-v", default="en-US-AndrewMultilingualNeural", help="TTS voice name"
    )
    syn_parser.add_argument(
        "--markers", "-m", help="Comma-separated marker offsets: 'intro:0.1,outro:9.8'"
    )

    conv_parser = subparsers.add_parser("convert", help="Convert JSON or SRT to .speech.json")
    conv_parser.add_argument("input", help="Input Whisper JSON, Universal Speech JSON, or SRT file")
    conv_parser.add_argument("--output", "-o", required=True, help="Output .speech.json path")

    args = parser.parse_args()

    if args.command == "synthesize":
        markers = parse_markers(args.markers) if args.markers else {}
        asyncio.run(
            synthesize_speech(
                text=args.text,
                output_prefix=args.output,
                voice=args.voice,
                markers_dict=markers,
            )
        )
    elif args.command == "convert":
        convert_transcript(args.input, args.output)


if __name__ == "__main__":
    main()
