#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = [
#     "edge-tts",
# ]
# ///
"""
Speech & Caption Synchronizer for Animatix.

Generates real audio (.mp3) and word-level speech transcripts (.speech.json)
for Animatix scenes, compatible with #voice@marker, #voice["word"], and
first-class `Captions` primitives.

Usage:
  # Text-to-Speech synthesis with word timestamps:
  uv run scripts/speech_sync.py synthesize \\
      --text "Welcome to Animatix. The layout-first animation DSL for technical storytelling." \\
      --markers "intro:0.1,feature:1.9,captions:6.2,outro:9.8" \\
      --output examples/assets/narration

  # Convert Whisper JSON / SRT to universal .speech.json:
  uv run scripts/speech_sync.py convert input.json -o output.speech.json
"""

import argparse
import asyncio
import json
import os
import re
import sys
from pathlib import Path

try:
    import edge_tts
except ImportError:
    edge_tts = None


async def synthesize_speech(
    text: str,
    output_prefix: str,
    voice: str = "en-US-AndrewMultilingualNeural",
    markers_dict: dict = None,
):
    if edge_tts is None:
        sys.exit("Error: `edge-tts` is required. Run with `uv run scripts/speech_sync.py`.")

    audio_path = f"{output_prefix}.mp3"
    json_path = f"{output_prefix}.speech.json"

    communicate = edge_tts.Communicate(text, voice)
    words = []
    audio_data = bytearray()

    async for chunk in communicate.stream():
        if chunk["type"] == "audio":
            audio_data.extend(chunk["data"])
        elif chunk["type"] == "WordBoundary":
            # offset and duration in 100ns units (ticks)
            start_s = chunk["offset"] / 10_000_000.0
            dur_s = chunk["duration"] / 10_000_000.0
            words.append({
                "word": chunk["text"],
                "start": round(start_s, 3),
                "end": round(start_s + dur_s, 3),
                "confidence": 1.0,
            })

    with open(audio_path, "wb") as f:
        f.write(audio_data)

    total_duration = words[-1]["end"] + 0.3 if words else 0.0

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


def convert_transcript(input_path: str, output_path: str):
    p = Path(input_path)
    if not p.exists():
        sys.exit(f"Error: input file '{input_path}' not found.")

    content = p.read_text(encoding="utf-8")
    data = json.loads(content)

    # If already universal or whisper format, normalize
    markers = data.get("markers", {})
    segments = []

    raw_segs = data.get("segments") or data.get("cues") or []
    for i, seg in enumerate(raw_segs):
        start = seg.get("start") or seg.get("start_s") or 0.0
        end = seg.get("end") or seg.get("end_s") or start
        text = seg.get("text", "").strip()
        words = []
        for w in seg.get("words", []):
            w_start = w.get("start") or w.get("start_s") or start
            w_end = w.get("end") or w.get("end_s") or w_start
            words.append({
                "word": w.get("word") or w.get("text", ""),
                "start": round(float(w_start), 3),
                "end": round(float(w_end), 3),
                "confidence": float(w.get("confidence") or w.get("probability") or 1.0),
            })
        segments.append({
            "id": i,
            "start": round(float(start), 3),
            "end": round(float(end), 3),
            "text": text,
            "words": words,
        })

    duration = float(data.get("duration") or (segments[-1]["end"] if segments else 0.0))

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

    conv_parser = subparsers.add_parser("convert", help="Convert Whisper JSON to .speech.json")
    conv_parser.add_argument("input", help="Input JSON file")
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
