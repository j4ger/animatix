#!/usr/bin/env python3
"""Local Qwen3-TTS CustomVoice and SenseVoice provider for Mandarin narration."""

import argparse
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import unicodedata

from speech_chapter import SAMPLE_RATE, json_bytes, read_json, signed_rate, write_wav


ASR_RATE = 16_000
LATIN_OR_DIGITS = re.compile(r"[A-Za-z0-9Ａ-Ｚａ-ｚ０-９]+")


def normalized(text):
    return "".join(char for char in unicodedata.normalize("NFKC", text).casefold() if char.isalnum())


def read_lexicon(path):
    if path is None:
        return {}
    source = read_json(path)
    if not isinstance(source, dict):
        raise ValueError("the lexicon must be an object mapping authored words to recognition aliases")
    lexicon = {}
    for word, aliases in source.items():
        if (not normalized(word) or not isinstance(aliases, list)
                or any(not isinstance(alias, str) or not normalized(alias) for alias in aliases)):
            raise ValueError(f"invalid recognition aliases for '{word}'")
        lexicon[word] = {normalized(alias) for alias in [word, *aliases]}
    return dict(sorted(lexicon.items(), key=lambda entry: -len(entry[0])))


def authored_words(text, lexicon):
    words = []
    index = 0
    while index < len(text):
        if not normalized(text[index]):
            index += 1
            continue
        word = next((term for term in lexicon if text.startswith(term, index)), None)
        if word is None:
            run = LATIN_OR_DIGITS.match(text, index)
            word = run.group() if run else text[index]
        words.append(word)
        index += len(word)
    return words


def recognized_characters(tokens, timestamps, duration):
    if len(tokens) != len(timestamps):
        raise ValueError("recognition returned different counts of tokens and timestamps")
    chars = []
    times = []
    previous = 0
    for token, start in zip(tokens, timestamps):
        if not math.isfinite(start) or not previous <= start < duration:
            raise ValueError(f"recognition returned an invalid timestamp for '{token}'")
        previous = start
        if re.fullmatch(r"<\|[^|]+\|>", token):
            continue  # SenseVoice metadata, such as its detected language.
        value = normalized(token)
        chars.append(value)
        times.extend([float(start)] * len(value))
    return "".join(chars), times


def match_words(text, tokens, timestamps, duration, lexicon):
    words = authored_words(text, lexicon)
    recognized, times = recognized_characters(tokens, timestamps, duration)
    if not words or not recognized:
        raise ValueError("the authored sentence and recognition output must both contain spoken text")
    boundaries = []
    aliases = []
    position = 0
    for word in words:
        plain = normalized(word)
        forms = lexicon.get(word, {plain})
        matches = [form for form in forms if recognized.startswith(form, position)]
        if len(matches) != 1:
            raise ValueError(f"could not align '{word}' at character {position + 1}; recognized {recognized!r}")
        boundaries.append(times[position])
        form = matches[0]
        if form != plain:
            aliases.append((word, form))
        position += len(form)
    if position != len(recognized):
        raise ValueError(f"recognition includes additional spoken text: {recognized[position:]!r}")
    boundaries.append(duration)

    aligned = []
    for word, start, end in zip(words, boundaries, boundaries[1:]):
        if end <= start:
            raise ValueError(f"recognition did not distinguish the boundary after '{word}'")
        aligned.append({"word": word, "start": start, "end": end})
    for word, alias in aliases:
        print(f"Accepted recognition alias {alias!r} for {word!r}.", file=sys.stderr)
    return aligned


def tempo_filters(rate):
    percent = int(signed_rate(rate)[:-1])
    tempo = (100 + percent) / 100
    filters = []
    while tempo < 0.5:
        filters.append("atempo=0.5")
        tempo *= 2
    while tempo > 2:
        filters.append("atempo=2")
        tempo /= 2
    if tempo != 1:
        filters.append(f"atempo={tempo:.12g}")
    return filters


def ffmpeg_audio(audio, in_format, in_rate, out_format, out_rate, filters=()):
    args = ["ffmpeg", "-nostdin", "-v", "error", "-xerror", "-f", in_format, "-ar", str(in_rate),
            "-ac", "1", "-i", "pipe:0"]
    if filters:
        args.extend(["-af", ",".join(filters)])
    args.extend(["-ac", "1", "-ar", str(out_rate), "-f", out_format, "pipe:1"])
    process = subprocess.run(args, input=audio, capture_output=True)
    if process.returncode != 0:
        raise ValueError(f"FFmpeg audio conversion failed: {process.stderr.decode(errors='replace').strip()}")
    return process.stdout


def synthesize_and_recognize(request, options, lexicon):
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"

    try:
        import numpy as np
        from qwen_tts import Qwen3TTSModel
        import sherpa_onnx
        import torch
    except ImportError as error:
        raise ValueError("install qwen-tts, PyTorch, and sherpa-onnx in the provider's environment") from error

    torch.set_num_threads(options.threads)
    torch.set_num_interop_threads(1)
    qwen = Qwen3TTSModel.from_pretrained(str(options.tts_model.resolve()), device_map="cpu",
                                        dtype=torch.bfloat16, attn_implementation="sdpa", local_files_only=True)
    text = request["text"]
    with torch.inference_mode():
        waveforms, sample_rate = qwen.generate_custom_voice(
            text=text, language="Chinese", speaker=request["voice"], non_streaming_mode=True,
            temperature=0.55, top_k=22, max_new_tokens=max(160, len(text) * 8),
        )
    if len(waveforms) != 1:
        raise ValueError("Qwen did not return exactly one take")
    waveform = np.asarray(waveforms[0], dtype="<f4")
    if waveform.ndim != 1 or not waveform.size or not np.isfinite(waveform).all():
        raise ValueError("Qwen did not return finite mono speech audio")

    # Decode timestamps from the final, tempo-adjusted take.
    pcm = ffmpeg_audio(waveform.tobytes(), "f32le", sample_rate, "s16le", SAMPLE_RATE,
                       tempo_filters(request["rate"]))
    if not pcm or len(pcm) % 2:
        raise ValueError("the tempo-adjusted take contains no complete PCM samples")
    recognition_audio = ffmpeg_audio(pcm, "s16le", SAMPLE_RATE, "f32le", ASR_RATE)
    recognizer = sherpa_onnx.OfflineRecognizer.from_sense_voice(
        tokens=str(options.asr_tokens), model=str(options.asr_model), language="zh",
        num_threads=options.threads, use_itn=False,
    )
    stream = recognizer.create_stream()
    stream.accept_waveform(ASR_RATE, np.frombuffer(recognition_audio, dtype="<f4"))
    recognizer.decode_stream(stream)
    result = stream.result
    duration = len(pcm) / 2 / SAMPLE_RATE
    words = match_words(text, result.tokens, result.timestamps, duration, lexicon)
    write_wav(Path(request["audio"]), [(pcm, 0)])
    Path(request["words"]).write_bytes(json_bytes(words))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tts-model", required=True, type=Path, help="Local Qwen3-TTS CustomVoice directory")
    parser.add_argument("--asr-model", required=True, type=Path, help="Local SenseVoice ONNX model")
    parser.add_argument("--asr-tokens", required=True, type=Path, help="Local SenseVoice tokens.txt")
    parser.add_argument("--lexicon", type=Path, help="Authored words and approved recognition aliases as JSON")
    parser.add_argument("--threads", type=int, default=4, help="CPU inference threads")
    options = parser.parse_args()
    if options.threads < 1:
        parser.error("--threads must be positive")
    if not options.tts_model.is_dir():
        parser.error("--tts-model must be a local directory")
    if not options.asr_model.is_file() or not options.asr_tokens.is_file():
        parser.error("--asr-model and --asr-tokens must be local files")
    try:
        lexicon = read_lexicon(options.lexicon)
        request = json.load(sys.stdin)
        synthesize_and_recognize(request, options, lexicon)
    except (KeyError, OSError, RuntimeError, TypeError, ValueError) as error:
        parser.exit(1, f"Qwen narration error: {error}\n")


if __name__ == "__main__":
    main()
