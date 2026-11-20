#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = ["edge-tts==7.3.1"]
# ///
"""Audition sentence reads and assemble sample-accurate Animatix narration."""

import argparse
import asyncio
from contextlib import contextmanager
from dataclasses import dataclass, replace
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import string
import subprocess
import tempfile
import wave

from speech_sync import collect_edge_speech


SAMPLE_RATE = 24_000
FORMAT = (1, 2, SAMPLE_RATE, "NONE")  # Mono, 16-bit PCM
DEFAULT_VOICE = "en-US-AndrewMultilingualNeural"
CACHE_VERSION = 1


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True, allow_nan=False) + "\n").encode()


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def digest(data):
    return hashlib.sha256(data).hexdigest()


def nonempty_text(value, label):
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{label} must be a nonempty string")
    return value.strip()


def seconds(value, label):
    if type(value) not in (float, int) or not math.isfinite(value) or value < 0:
        raise ValueError(f"{label} must be a finite, nonnegative number")
    return value


def signed_rate(value):
    if not isinstance(value, str) or not re.fullmatch(r"[+-][0-9]+%", value):
        raise ValueError("rate must be a signed percentage such as +0% or -12%")
    percent = int(value[:-1])
    if percent <= -100:
        raise ValueError("rate must be greater than -100%")
    return f"{percent:+d}%"


@dataclass(frozen=True)
class Sentence:
    id: str
    text: str
    voice: str
    rate: str
    pause: float
    markers: dict

    def request(self):
        return {"text": self.text, "voice": self.voice, "rate": self.rate}


def read_chapter(path, allow_empty=False):
    source = read_json(path)
    if not isinstance(source, dict) or not isinstance(source.get("sentences"), list):
        raise ValueError("chapter requires a 'sentences' array")
    if not allow_empty and not source["sentences"]:
        raise ValueError("chapter requires a nonempty 'sentences' array")
    sentences = []
    ids = set()
    marker_names = set()
    for item in source["sentences"]:
        if not isinstance(item, dict):
            raise ValueError("each chapter sentence must be an object")
        name = nonempty_text(item.get("id"), "sentence id")
        if name in ids:
            raise ValueError(f"duplicate sentence id '{name}'")
        ids.add(name)
        markers = item.get("markers", {})
        if not isinstance(markers, dict):
            raise ValueError(f"markers of sentence '{name}' must be an object")
        for marker, target in markers.items():
            if not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", marker) or marker in marker_names:
                raise ValueError(f"invalid or duplicate marker name '{marker}'")
            marker_names.add(marker)
            if isinstance(target, str):
                nonempty_text(target, f"word for marker '{marker}'")
            elif (not isinstance(target, dict) or type(target.get("occurrence", 1)) is not int
                  or target.get("occurrence", 1) < 1):
                raise ValueError(f"marker '{marker}' requires a word and a positive occurrence")
            else:
                nonempty_text(target.get("word"), f"word for marker '{marker}'")
        sentences.append(Sentence(
            name, nonempty_text(item.get("text"), f"text of '{name}'"),
            nonempty_text(item.get("voice", source.get("voice", DEFAULT_VOICE)), f"voice of '{name}'"),
            signed_rate(item.get("rate", source.get("rate", "+0%"))),
            seconds(item.get("pause_after", source.get("pause_after", 0)), f"pause after '{name}'"),
            markers,
        ))
    return sentences


def read_pcm(path):
    with wave.open(str(path), "rb") as clip:
        if (clip.getnchannels(), clip.getsampwidth(), clip.getframerate(), clip.getcomptype()) != FORMAT:
            raise ValueError("audio must be mono 24 kHz, 16-bit PCM WAV")
        size = clip.getnframes() * FORMAT[1]
        pcm = clip.readframes(clip.getnframes())
        if not pcm or len(pcm) != size:
            raise ValueError("audio is empty or truncated")
        return pcm


def write_wav(path, clips):
    with wave.open(str(path), "wb") as output:
        output.setparams((*FORMAT[:3], 0, FORMAT[3], "not compressed"))
        for pcm, pause_frames in clips:
            output.writeframesraw(pcm)
            output.writeframesraw(bytes(pause_frames * FORMAT[1]))


def normalize_audio(path, destination=None):
    try:
        return read_pcm(path)
    except (wave.Error, EOFError, ValueError):
        normalized = destination if destination is not None else path.with_name("normalized.wav")
        subprocess.run([
            "ffmpeg", "-nostdin", "-v", "error", "-xerror", "-y", "-i", str(path),
            "-map", "0:a:0", "-ac", "1", "-ar", str(SAMPLE_RATE), "-c:a", "pcm_s16le",
            str(normalized),
        ], check=True)
        return read_pcm(normalized)


def validate_words(raw_words, audio_frames):
    if not isinstance(raw_words, list) or not raw_words:
        raise ValueError("synthesis requires a nonempty list of word boundaries")
    words = []
    previous_start = 0
    for raw in raw_words:
        if not isinstance(raw, dict):
            raise ValueError("word boundary must be an object")
        value = nonempty_text(raw.get("word"), "word boundary text")
        start_s = seconds(raw.get("start"), f"start of '{value}'")
        end_s = seconds(raw.get("end"), f"end of '{value}'")
        start = round(start_s * SAMPLE_RATE)
        end = round(end_s * SAMPLE_RATE)
        if start_s > end_s or not previous_start <= start <= end <= audio_frames:
            raise ValueError(f"invalid or out-of-range word boundary for '{value}'")
        word = {"word": value, "start": start / SAMPLE_RATE, "end": end / SAMPLE_RATE}
        if "confidence" in raw:
            confidence = seconds(raw["confidence"], f"confidence of '{value}'")
            if confidence > 1:
                raise ValueError(f"confidence of '{value}' must be between 0 and 1")
            word["confidence"] = confidence
        words.append(word)
        previous_start = start
    return words


@dataclass(frozen=True)
class Provider:
    command: tuple
    cwd: Path
    revision: str

    def input_id(self, sentence):
        identity = {"sentence": sentence.request(), "command": self.command,
                    "cwd": str(self.cwd) if self.command else "", "revision": self.revision}
        return digest(json_bytes(identity))

    def synthesize(self, sentence, scratch):
        try:
            if self.command:
                audio = scratch / "source.wav"
                timing = scratch / "words.json"
                request = {**sentence.request(), "audio": str(audio), "words": str(timing)}
                subprocess.run(self.command, cwd=self.cwd, input=json_bytes(request), check=True)
                raw_words = read_json(timing)
            else:
                audio = scratch / "source.mp3"
                audio_data, raw_words = asyncio.run(collect_edge_speech(**sentence.request()))
                audio.write_bytes(audio_data)
            pcm = normalize_audio(audio)
            words = validate_words(raw_words, len(pcm) // FORMAT[1])
            return Take(self.input_id(sentence), pcm, words)
        except (OSError, ValueError, EOFError, wave.Error, subprocess.SubprocessError) as error:
            raise ValueError(f"sentence '{sentence.id}': {error}") from error


@dataclass(frozen=True)
class Take:
    input_id: str
    pcm: bytes
    words: list

    def metadata(self):
        data = {"input": self.input_id, "pcm": digest(self.pcm), "words": self.words}
        return {**data, "take": digest(json_bytes(data))}


def read_take(record, file_for_pcm, input_id):
    try:
        if record["input"] != input_id or not re.fullmatch(r"[0-9a-f]{64}", record["pcm"]):
            return None
        pcm = read_pcm(file_for_pcm(record["pcm"]))
        take = Take(input_id, pcm, validate_words(record["words"], len(pcm) // FORMAT[1]))
        return take if record == take.metadata() else None
    except (KeyError, TypeError, ValueError, OSError, EOFError, wave.Error):
        return None


def checkpoint_path(cache, sentence, input_id):
    key = digest(json_bytes([CACHE_VERSION, sentence.id, input_id]))
    return cache / f"request-{key}.json"


def read_checkpoint(cache, sentence, input_id):
    try:
        return read_json(checkpoint_path(cache, sentence, input_id))
    except (ValueError, OSError):
        return None


def cache_take(cache, scratch, sentence, take, checkpoint):
    record = take.metadata()
    staged_clip = scratch / "take.wav"
    write_wav(staged_clip, [(take.pcm, 0)])
    staged_clip.replace(cache / f"{record['pcm']}.wav")
    if checkpoint:
        staged_request = scratch / "request.json"
        staged_request.write_bytes(json_bytes(record))
        staged_request.replace(checkpoint_path(cache, sentence, take.input_id))


def read_audition_take(directory, index, sentence, provider):
    try:
        audition = read_json(directory / "audition.json")
        if audition["sentence"] == sentence.id:
            record = audition["candidates"][index - 1]["take"]
            path = directory / f"candidate-{index:02}.wav"
            take = read_take(record, lambda _: path, provider.input_id(sentence))
            if take is not None:
                return take
    except (KeyError, IndexError, TypeError, ValueError, OSError):
        pass
    raise ValueError("audition take is unavailable or does not match the sentence text, voice, rate, and provider")


WORD_EDGES = re.compile(r"^[\s" + re.escape(string.punctuation) + r"]+|[\s" + re.escape(string.punctuation) + r"]+$")
ASCII_LOWERCASE = str.maketrans(string.ascii_uppercase, string.ascii_lowercase)


def clean_word(word):
    return WORD_EDGES.sub("", word).translate(ASCII_LOWERCASE)


def resolve_marker(name, target, words):
    query, occurrence = (target, 1) if isinstance(target, str) else (target["word"], target.get("occurrence", 1))
    query = clean_word(query)
    for word in words:
        if query and query == clean_word(word["word"]):
            occurrence -= 1
            if occurrence == 0:
                return word["start"]
    raise ValueError(f"marker '{name}' did not match a spoken word")


def speech_segment(sentence, take, offset=0, index=0):
    words = [{**word, "start": round(offset / SAMPLE_RATE + word["start"], 6),
              "end": round(offset / SAMPLE_RATE + word["end"], 6)} for word in take.words]
    return {"id": index, "start": words[0]["start"], "end": max(word["end"] for word in words),
            "text": sentence.text, "words": words}


def assemble(sentences, takes, audio):
    offset = 0
    segments = []
    markers = {}
    clips = []
    for index, sentence in enumerate(sentences):
        take = takes[sentence.id]
        pause = round(sentence.pause * SAMPLE_RATE)
        segment = speech_segment(sentence, take, offset, index)
        segments.append(segment)
        for name, target in sentence.markers.items():
            markers[name] = resolve_marker(name, target, segment["words"])
        clips.append((take.pcm, pause))
        offset += len(take.pcm) // FORMAT[1] + pause
    write_wav(audio, clips)
    return {"duration": round(offset / SAMPLE_RATE, 6), "markers": markers, "segments": segments}


def marker_report(previous, current):
    changes = []
    for name in sorted(previous.keys() | current.keys()):
        before, after = previous.get(name), current.get(name)
        if before != after:
            changes.append({"marker": name, "before": before, "after": after,
                            "delta": round(after - before, 6) if before is not None and after is not None else None})
    return changes


@contextmanager
def output_lock(cache):
    lock_path = cache / "write.lock"
    # The OS releases the lock even if the writer is interrupted.
    with lock_path.open("a+b") as lock:
        try:
            if os.name == "nt":
                import msvcrt

                lock.seek(0)
                msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl

                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as error:
            raise ValueError(f"could not lock {lock_path} for a chapter write: {error}") from error
        yield


def publish(scratch, outputs):
    backups = []
    for number, (_, target) in enumerate(outputs):
        backup = scratch / f"backup-{number}"
        if target.exists():
            shutil.copyfile(target, backup)
        backups.append(backup)
    published = 0
    try:
        for staged, target in outputs:
            staged.replace(target)
            published += 1
    except BaseException:
        for (_, target), backup in zip(outputs[:published], backups):
            if backup.exists():
                backup.replace(target)
            else:
                target.unlink()
        raise


def read_receipt(path):
    try:
        data = read_json(path)
        if data["version"] == CACHE_VERSION and isinstance(data["takes"], dict):
            if isinstance(data["order"], list) and isinstance(data["markers"], dict):
                for marker, time in data["markers"].items():
                    seconds(time, marker)
                return data
    except (KeyError, TypeError, ValueError, OSError):
        pass  # A chapter build can regenerate missing or damaged cached state.
    return {"order": [], "takes": {}, "markers": {}}


def build_chapter(sentences, provider, prefix, selected=None, from_audition=None):
    order = [sentence.id for sentence in sentences]
    if selected is not None and selected not in order:
        raise ValueError(f"unknown sentence '{selected}'")
    cache = prefix.with_name(prefix.name + ".takes")
    wav = prefix.with_name(prefix.name + ".wav")
    transcript = prefix.with_name(prefix.name + ".speech.json")
    cache.mkdir(parents=True, exist_ok=True)
    with output_lock(cache), tempfile.TemporaryDirectory(dir=cache) as temp:
        scratch = Path(temp)
        receipt = cache / "receipt.json"
        previous = read_receipt(receipt)
        if selected is not None and previous["order"] != order:
            raise ValueError("retake requires a previously built chapter with the same sentence IDs and order")

        takes = {}
        for sentence in sentences:
            if sentence.id == selected:
                continue
            input_id = provider.input_id(sentence)
            record = previous["takes"].get(sentence.id)
            take = read_take(record, lambda key: cache / f"{key}.wav", input_id)
            if take is None and selected is None:
                pending = read_checkpoint(cache, sentence, input_id)
                take = read_take(pending, lambda key: cache / f"{key}.wav", input_id)
            if take is None and selected is not None:
                raise ValueError(f"cannot reuse sentence '{sentence.id}'; build the chapter before retaking")
            if take is not None:
                takes[sentence.id] = take
        reused = set(takes)
        for sentence in sentences:
            if sentence.id not in takes:
                if sentence.id == selected and from_audition is not None:
                    directory, index = from_audition
                    takes[sentence.id] = read_audition_take(directory, index, sentence, provider)
                else:
                    with tempfile.TemporaryDirectory(dir=scratch) as clip_dir:
                        takes[sentence.id] = provider.synthesize(sentence, Path(clip_dir))
                cache_take(cache, scratch, sentence, takes[sentence.id], checkpoint=selected is None)

        staged_audio = scratch / "chapter.wav"
        assembled = assemble(sentences, takes, staged_audio)
        report = marker_report(previous["markers"], assembled["markers"])
        records = {key: take.metadata() for key, take in takes.items()}
        stage_transcript = scratch / "chapter.speech.json"
        stage_transcript.write_bytes(json_bytes(assembled))
        stage_receipt = scratch / "receipt.json"
        stage_receipt.write_bytes(json_bytes({"version": CACHE_VERSION, "order": order,
                                              "markers": assembled["markers"], "takes": records}))
        publish(scratch, [(staged_audio, wav), (stage_transcript, transcript), (stage_receipt, receipt)])
    adopted = int(from_audition is not None)
    counts = f"{len(takes) - len(reused) - adopted} synthesized, {len(reused)} reused"
    if adopted:
        counts += ", 1 audition take applied"
    print(f"Wrote {wav} and {transcript} ({counts}).")
    if selected is not None:
        print(json_bytes({"sentence": selected, "affected_markers": report}).decode().strip())


def audition(sentences, provider, selected, candidates, output):
    sentence = next((sentence for sentence in sentences if sentence.id == selected), None)
    if sentence is None:
        raise ValueError(f"unknown sentence '{selected}'")
    reads = []
    for candidate in candidates:
        voice, separator, rate = candidate.rpartition("@")
        if not separator:
            voice, rate = candidate, sentence.rate
        reads.append(replace(sentence, voice=nonempty_text(voice, "candidate voice"), rate=signed_rate(rate)))
    if output.exists():
        raise ValueError(f"audition output already exists: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=output.parent) as temp:
        scratch = Path(temp)
        bundle = scratch / "audition"
        bundle.mkdir()
        entries = []
        clips = []
        offset = 0
        for index, read in enumerate(reads, start=1):
            with tempfile.TemporaryDirectory(dir=scratch) as clip_dir:
                take = provider.synthesize(read, Path(clip_dir))
            filename = f"candidate-{index:02}.wav"
            pause = SAMPLE_RATE // 2 if index < len(reads) else 0
            write_wav(bundle / filename, [(take.pcm, 0)])
            entries.append({"audio": filename, "voice": read.voice, "rate": read.rate,
                            "comparison_start": round(offset / SAMPLE_RATE, 6), "take": take.metadata()})
            clips.append((take.pcm, pause))
            offset += len(take.pcm) // FORMAT[1] + pause
        write_wav(bundle / "comparison.wav", clips)
        (bundle / "audition.json").write_bytes(json_bytes({"sentence": selected, "text": sentence.text,
                                                          "candidates": entries}))
        bundle.rename(output)
    for entry in entries:
        print(f"{output / entry['audio']}  {entry['voice']} {entry['rate']}")
    print(f"Play all candidates in order from {output / 'comparison.wav'}.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for command, description in [
        ("build", "Assemble a chapter and synthesize changed or missing sentences"),
        ("audition", "Compare reads of the same sentence with different voices or rates"),
        ("retake", "Synthesize one sentence and report changed marker times"),
    ]:
        action = commands.add_parser(command, help=description)
        action.add_argument("manifest", type=Path, help="Chapter JSON file")
        action.add_argument("--output", "-o", type=Path, required=True,
                            help="New audition directory" if command == "audition" else "Chapter output prefix")
        action.add_argument("--provider-command", help="Local provider executable (default: Edge TTS)")
        action.add_argument("--provider-arg", action="append", default=[], help="Argument to the local provider")
        action.add_argument("--provider-revision", default="", help="Cache revision for provider or model changes")
        if command != "build":
            action.add_argument("sentence", help="Sentence ID from the chapter")
        if command == "audition":
            action.add_argument("--candidate", action="append", required=True,
                                help="Candidate VOICE or VOICE@RATE, repeat to compare")
        if command == "retake":
            action.add_argument("--from-audition", type=Path, help="Apply a take from this audition directory")
            action.add_argument("--candidate", type=int, help="1-based candidate number from that audition")
    args = parser.parse_args()
    if args.provider_arg and not args.provider_command:
        parser.error("--provider-arg requires --provider-command")
    from_audition = None
    if args.command == "retake":
        if args.from_audition is not None:
            if args.candidate is None or args.candidate < 1:
                parser.error("--from-audition requires a positive --candidate number")
            from_audition = (args.from_audition, args.candidate)
        elif args.candidate is not None:
            parser.error("--candidate requires --from-audition for a retake")
    manifest = args.manifest.resolve()
    output = args.output.resolve()
    if args.command != "audition":
        if not output.name:
            parser.error("chapter output prefix must include a filename")
        if manifest in (output.with_name(output.name + suffix) for suffix in (".speech.json", ".wav")):
            parser.error("chapter manifest and output must be different files")
    command = tuple([args.provider_command, *args.provider_arg]) if args.provider_command else ()
    provider = Provider(command, manifest.parent, args.provider_revision)
    try:
        sentences = read_chapter(manifest)
        if args.command == "audition":
            audition(sentences, provider, args.sentence, args.candidate, output)
        else:
            build_chapter(sentences, provider, output, getattr(args, "sentence", None), from_audition)
    except (ValueError, OSError, OverflowError, wave.Error) as error:
        parser.exit(1, f"Error: {error}\n")


if __name__ == "__main__":
    main()
