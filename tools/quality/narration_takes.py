#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = ["edge-tts==7.3.1"]
# ///
"""Prepare, preview, and adopt paragraph takes for Animatix narration."""

import argparse
from dataclasses import replace
import json
from pathlib import Path
import re
import subprocess
import tempfile
from uuid import uuid4
import wave

from narration_timeline import (
    Chapter, ParagraphTake, Region, audio_slice, frames, from_external, rebase_markers, sidecar_words,
    word_fields,
)
from qwen_chapter_provider import normalized, read_lexicon, recognize_pcm
from speech_chapter import (
    Provider, SAMPLE_RATE, Take, assemble, digest, json_bytes, marker_report, nonempty_text,
    normalize_audio, output_lock, read_chapter, read_json, read_pcm, read_receipt, read_take, resolve_marker,
    signed_rate, speech_segment, validate_words, write_wav,
)


VERSION = 1
IDENTIFIER = re.compile(r"[0-9a-f]{32}")
WAV = "chapter.wav"
TRANSCRIPT = "chapter.speech.json"


class Store:
    def __init__(self, prefix):
        self.prefix = prefix.resolve()
        if not self.prefix.name:
            raise ValueError("chapter output prefix must include a filename")
        self.cache = self.suffixed(".takes")
        self.candidates = self.cache / "candidates"
        self.revisions = self.cache / "revisions"
        self.current = self.cache / "current.json"

    def suffixed(self, suffix):
        return self.prefix.with_name(self.prefix.name + suffix)

    def baseline(self, sentences, audio, transcript, empty=False):
        if (audio is None) != (transcript is None):
            raise ValueError("--base-audio and --base-transcript must be supplied together")
        if empty:
            if audio is not None:
                raise ValueError("--empty-base cannot be combined with existing chapter assets")
            return Chapter([sentence.id for sentence in sentences], {}), "empty", None, None, {}
        if audio is None:
            if self.current.exists():
                identifier = revision_id(read_json(self.current)["revision"])
                directory = self.revisions / identifier
                return self.read_revision(directory)
            fixed_audio = self.suffixed(".wav")
            fixed_transcript = self.suffixed(".speech.json")
            if not fixed_audio.exists() and not fixed_transcript.exists():
                return Chapter([sentence.id for sentence in sentences], {}), "empty", None, None, {}
            audio, transcript = fixed_audio, fixed_transcript
        audio, transcript = audio.resolve(), transcript.resolve()
        in_revisions = (self.revisions.resolve() in audio.parents, self.revisions.resolve() in transcript.parents)
        if any(in_revisions):
            if audio.name != WAV or transcript.name != TRANSCRIPT or audio.parent != transcript.parent:
                raise ValueError("chapter audio and transcript must belong to the same revision")
            return self.read_revision(audio.parent)
        try:
            transcript_bytes = transcript.read_bytes()
            raw = json.loads(transcript_bytes.decode("utf-8-sig"))
            with tempfile.TemporaryDirectory() as scratch:
                pcm = normalize_audio(audio, Path(scratch) / "normalized.wav")
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            raise ValueError(f"could not read the existing audio and transcript: {error}") from error
        revision = digest(json_bytes([str(audio), str(transcript), digest(pcm), digest(transcript_bytes)]))
        ranges = self.legacy_ranges(raw, pcm, audio, transcript)
        chapter = from_external(sentences, pcm, raw, ranges)
        return chapter, revision, audio, transcript, raw.get("markers", {})

    def legacy_ranges(self, raw, pcm, audio, transcript):
        if audio != self.suffixed(".wav") or transcript != self.suffixed(".speech.json"):
            return None
        receipt = read_receipt(self.cache / "receipt.json")
        segments = raw.get("segments", []) if isinstance(raw, dict) else []
        if not receipt["order"] or len(receipt["order"]) != len(segments):
            return None
        spans = []
        for name, segment in zip(receipt["order"], segments):
            record = receipt["takes"].get(name)
            if not isinstance(record, dict):
                return None
            take = read_take(record, lambda key: self.cache / f"{key}.wav", record.get("input"))
            if take is None:
                return None
            try:
                start = round((segment["start"] - take.words[0]["start"]) * SAMPLE_RATE)
            except (KeyError, TypeError, ValueError):
                return None
            end = start + frames(take.pcm)
            if start < 0 or audio_slice(pcm, start, end) != take.pcm or (spans and start < spans[-1][1]):
                return None
            spans.append((start, end))
        return spans

    def read_revision(self, directory):
        identifier = revision_id(directory.name)
        if directory.resolve().parent != self.revisions.resolve():
            raise ValueError("chapter revision path is outside the take store")
        audio, transcript = directory / WAV, directory / TRANSCRIPT
        try:
            record = read_json(directory / "state.json")
            pcm = read_pcm(audio)
            raw = transcript.read_bytes()
            if record["version"] != VERSION or record["pcm"] != digest(pcm) or record["timing"] != digest(raw):
                raise ValueError("revision audio or transcript was modified")
            return Chapter.from_state(record["chapter"], pcm), identifier, audio, transcript, json.loads(raw)["markers"]
        except (KeyError, TypeError, OSError, ValueError, EOFError, wave.Error) as error:
            raise ValueError(f"cannot read chapter revision {identifier}: {error}") from error

    def write_revision(self, scratch, chapter, make_current=True):
        revision = uuid4().hex
        bundle = scratch / "chapter"
        bundle.mkdir()
        chunks, timing = chapter.assemble()
        write_wav(bundle / WAV, chunks)
        timing_bytes = json_bytes(timing)
        (bundle / TRANSCRIPT).write_bytes(timing_bytes)
        (bundle / "state.json").write_bytes(json_bytes({
            "version": VERSION, "pcm": digest(read_pcm(bundle / WAV)), "timing": digest(timing_bytes),
            "chapter": chapter.state(),
        }))
        self.revisions.mkdir(exist_ok=True)
        published = self.revisions / revision
        bundle.rename(published)
        if make_current:
            staged_pointer = scratch / "current.json"
            staged_pointer.write_bytes(json_bytes({"revision": revision}))
            staged_pointer.replace(self.current)
        return revision, published / WAV, published / TRANSCRIPT, timing

    def candidate(self, path):
        path = path.resolve()
        if path.name != "take.json" or path.parent.parent != self.candidates.resolve():
            raise ValueError("candidate is outside this chapter's take store")
        revision_id(path.parent.name)
        record = read_json(path)
        if record["version"] != VERSION:
            raise ValueError("unsupported candidate take format")
        return path, record

    def candidate_index(self, sentences):
        result = {sentence.id: [] for sentence in sentences}
        expected = {sentence.id: sentence.text for sentence in sentences}
        records = sorted(self.candidates.glob("*/take.json"), key=lambda path: path.parent.name)
        for path in records:
            try:
                path, record = self.candidate(path)
                name = record["paragraph"]
                if record["text"] != expected.get(name) or not path.with_name("take.wav").is_file():
                    continue
                if path.with_name("take.speech.json").is_file():
                    result[name].append(review(path, record))
            except (KeyError, OSError, ValueError, TypeError):
                continue  # A partial or invalid audition is never offered to the editor.
        return result


def revision_id(value):
    if not isinstance(value, str) or not IDENTIFIER.fullmatch(value):
        raise ValueError("invalid take revision identifier")
    return value


def review(path, record):
    return {
        "candidate": str(path), "audio": str(path.with_name("take.wav")),
        "transcript": str(path.with_name("take.speech.json")),
        "paragraph": record["paragraph"], "voice": record["voice"], "rate": record["rate"],
        "kind": record["kind"], "duration": record["duration"], "word_count": len(record["take"]["words"]),
    }


def imported_take(sentence, args, scratch):
    pcm = normalize_audio(args.audio.resolve(), scratch / "normalized.wav")
    if args.timing is not None:
        raw = json.loads(args.timing.read_text(encoding="utf-8-sig"))
        raw_words = [word_fields(word) for word in sidecar_words(raw)]
    else:
        raw_words = recognize_pcm(sentence.text, pcm, args, read_lexicon(args.lexicon))
    words = validate_words(raw_words, frames(pcm))
    actual = "".join(normalized(word["word"]) for word in words)
    if not actual or actual != normalized(sentence.text):
        raise ValueError("aligned recording words do not match the authored paragraph")
    input_id = digest(json_bytes({"recording": digest(pcm), "text": sentence.text}))
    return Take(input_id, pcm, words)


def prepare(store, sentence, args, manifest):
    store.candidates.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=store.cache) as directory:
        scratch = Path(directory)
        if args.command == "synthesize":
            command = tuple([args.provider_command, *args.provider_arg]) if args.provider_command else ()
            provider = Provider(command, manifest.parent, args.provider_revision)
            voice = nonempty_text(args.voice, "candidate voice") if args.voice is not None else sentence.voice
            rate = signed_rate(args.rate) if args.rate is not None else sentence.rate
            read = replace(sentence, voice=voice, rate=rate)
            take = provider.synthesize(read, scratch)
            kind = "synthesized"
        else:
            read = sentence
            take = imported_take(read, args, scratch)
            voice, rate, kind = None, None, "recorded"
        bundle = scratch / "candidate"
        bundle.mkdir()
        timing = assemble([replace(read, pause=0)], {read.id: take}, bundle / "take.wav")
        timing_bytes = json_bytes(timing)
        (bundle / "take.speech.json").write_bytes(timing_bytes)
        record = {"version": VERSION, "paragraph": read.id, "text": read.text, "voice": voice, "rate": rate,
                  "kind": kind, "duration": timing["duration"], "timing": digest(timing_bytes),
                  "take": take.metadata()}
        (bundle / "take.json").write_bytes(json_bytes(record))
        published = store.candidates / uuid4().hex
        bundle.rename(published)
    return review(published / "take.json", record)


def candidate_region(path, record, sentence):
    if record["paragraph"] != sentence.id or record["text"] != sentence.text:
        raise ValueError("candidate is stale for this paragraph; prepare a new take")
    take = read_take(record["take"], lambda _: path.with_name("take.wav"), record["take"]["input"])
    if take is None or record["timing"] != digest(path.with_name("take.speech.json").read_bytes()):
        raise ValueError("candidate audio or alignment was modified")
    segment = speech_segment(sentence, take)
    markers = {name: resolve_marker(name, target, segment["words"]) for name, target in sentence.markers.items()}
    return Region(take.pcm, [segment], markers)


def adopt(store, sentences, sentence, args):
    store.cache.mkdir(parents=True, exist_ok=True)
    with output_lock(store.cache), tempfile.TemporaryDirectory(dir=store.cache) as directory:
        scratch = Path(directory)
        chapter, previous, _, _, previous_markers = store.baseline(
            sentences, args.base_audio, args.base_transcript, args.empty_base)
        if args.expected_revision is not None and previous != args.expected_revision:
            raise ValueError("the chapter changed since review; refresh the takes and choose again")
        chapter.reconcile(sentences)
        path, record = store.candidate(args.candidate)
        region = candidate_region(path, record, sentence)
        old = chapter.takes.get(sentence.id)
        region.markers = {**rebase_markers(old, region), **region.markers}
        # Explicit definitions take precedence over marks inherited from a prior transcript.
        for name in sentence.markers:
            chapter.leading.markers.pop(name, None)
            for take in chapter.takes.values():
                take.speech.markers.pop(name, None)
                take.after.markers.pop(name, None)
        chapter.takes[sentence.id] = ParagraphTake(
            sentence.text, region, old.after if old else Region(), record["voice"], record["rate"],
            path.parent.name, round(sentence.pause * SAMPLE_RATE), tuple(sentence.markers),
        )
        revision, audio, transcript, timing = store.write_revision(
            scratch, chapter, make_current=not args.empty_base and args.base_audio is None)
    return {"paragraph": sentence.id, "revision": revision, "audio": str(audio), "transcript": str(transcript),
            "affected_markers": marker_report(previous_markers, timing["markers"])}


def inspect(store, sentences, args):
    chapter, revision, audio, transcript, _ = store.baseline(
        sentences, args.base_audio, args.base_transcript, args.empty_base)
    chapter.reconcile(sentences)
    candidates = store.candidate_index(sentences)
    paragraphs = []
    for sentence in sentences:
        active = chapter.takes.get(sentence.id)
        selected = store.candidates / active.candidate / "take.json" if active and active.candidate else None
        status = "missing" if active is None else "ready" if active.text == sentence.text else "stale"
        paragraphs.append({
            "id": sentence.id, "text": sentence.text, "voice": sentence.voice, "rate": sentence.rate,
            "status": status, "active_take": str(selected) if selected else None,
            "active_voice": active.voice if active else None, "active_rate": active.rate if active else None,
            "candidates": candidates[sentence.id],
        })
    return {"revision": revision, "audio": str(audio) if audio else None,
            "transcript": str(transcript) if transcript else None, "paragraphs": paragraphs}


def parser_for_worker():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("inspect", "synthesize", "import", "adopt"):
        action = commands.add_parser(name)
        action.add_argument("manifest", type=Path, help="Chapter JSON file with a sentence entry per paragraph")
        if name != "inspect":
            action.add_argument("paragraph", help="Paragraph ID from the chapter manifest")
        action.add_argument("--output", "-o", type=Path, required=True, help="Stable chapter output prefix")
        action.add_argument("--base-audio", type=Path, help="Currently authored chapter audio path")
        action.add_argument("--base-transcript", type=Path, help="Currently authored chapter transcript path")
        action.add_argument("--empty-base", action="store_true", help="Use an empty chapter regardless of saved takes")
        if name == "synthesize":
            action.add_argument("--voice", help="Candidate voice override")
            action.add_argument("--rate", help="Candidate signed speed change, such as --rate=-12%%")
            action.add_argument("--provider-command", help="Local synthesize-and-align executable (default: Edge TTS)")
            action.add_argument("--provider-arg", action="append", default=[], help="Local synthesis argument")
            action.add_argument("--provider-revision", default="", help="Model or synthesis configuration revision")
        elif name == "import":
            action.add_argument("--audio", required=True, type=Path, help="Recorded paragraph audio")
            alignment = action.add_mutually_exclusive_group(required=True)
            alignment.add_argument("--timing", type=Path, help="Word timing JSON sidecar")
            alignment.add_argument("--asr-model", type=Path, help="Local SenseVoice ONNX model")
            action.add_argument("--asr-tokens", type=Path, help="Local SenseVoice token file")
            action.add_argument("--lexicon", type=Path, help="Local SenseVoice alignment lexicon")
            action.add_argument("--threads", type=int, default=4, help="Local alignment CPU inference threads")
        elif name == "adopt":
            action.add_argument("--candidate", required=True, type=Path, help="Candidate take.json file")
            action.add_argument("--expected-revision", help="Chapter revision from the editor's last review")
    return parser


def main():
    parser = parser_for_worker()
    args = parser.parse_args()
    if args.command == "synthesize" and args.provider_arg and not args.provider_command:
        parser.error("--provider-arg requires --provider-command")
    if args.command == "import":
        if (args.asr_model is None) != (args.asr_tokens is None) or args.lexicon and not args.asr_model:
            parser.error("local alignment requires --asr-model and --asr-tokens together")
        if args.threads < 1:
            parser.error("--threads must be positive")
    try:
        manifest = args.manifest.resolve()
        store = Store(args.output)
        sentences = read_chapter(manifest, allow_empty=args.command == "inspect")
        if args.command == "inspect":
            result = inspect(store, sentences, args)
        else:
            sentence = next((entry for entry in sentences if entry.id == args.paragraph), None)
            if sentence is None:
                raise ValueError(f"unknown paragraph '{args.paragraph}'")
            if args.command == "adopt":
                result = adopt(store, sentences, sentence, args)
            else:
                result = prepare(store, sentence, args, manifest)
        print(json.dumps(result, ensure_ascii=True, allow_nan=False), flush=True)
    except (KeyError, TypeError, ValueError, OSError, OverflowError, EOFError, RuntimeError,
            wave.Error, subprocess.SubprocessError) as error:
        parser.exit(1, f"Narration error: {error}\n")


if __name__ == "__main__":
    main()
