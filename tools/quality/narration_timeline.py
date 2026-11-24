"""Sample-based layout for chapter takes and existing word-timed narration."""

from dataclasses import dataclass, field
from difflib import SequenceMatcher
import re

from qwen_chapter_provider import normalized
from speech_chapter import FORMAT, SAMPLE_RATE, seconds, validate_words


def frames(pcm):
    return len(pcm) // FORMAT[1]


def audio_slice(pcm, start, end):
    return pcm[start * FORMAT[1]:end * FORMAT[1]]


def shifted_time(time, offset):
    return round(time + offset / SAMPLE_RATE, 6)


def shifted_segments(segments, offset):
    def shifted(record):
        return {**record, "start": shifted_time(record["start"], offset),
                "end": shifted_time(record["end"], offset)}

    return [{**shifted(segment), "words": [shifted(word) for word in segment["words"]]}
            for segment in segments]


@dataclass
class Region:
    pcm: bytes = b""
    segments: list = field(default_factory=list)
    markers: dict = field(default_factory=dict)

    def state(self):
        return {"frames": frames(self.pcm), "segments": self.segments, "markers": self.markers}


@dataclass
class ParagraphTake:
    text: str
    speech: Region
    after: Region = field(default_factory=Region)
    voice: str = None
    rate: str = None
    candidate: str = None
    pause: int = 0
    authored_markers: tuple = ()
    unmapped: bool = False

    def state(self):
        return {**vars(self), "speech": self.speech.state(), "after": self.after.state()}


@dataclass
class Chapter:
    order: list
    takes: dict
    leading: Region = field(default_factory=Region)

    def assemble(self):
        chunks = []
        segments = []
        markers = {}
        takes = {}
        managed = False
        offset = 0

        def add(region, pause=0):
            nonlocal offset
            chunks.append((region.pcm, pause))
            segments.extend(shifted_segments(region.segments, offset))
            markers.update({name: shifted_time(time, offset) for name, time in region.markers.items()})
            offset += frames(region.pcm) + pause

        add(self.leading)
        for name in self.order:
            take = self.takes.get(name)
            is_managed = re.fullmatch(r"p_[0-9a-f]{16}_[1-9][0-9]*", name) is not None
            managed |= is_managed
            if take is not None:
                start = len(segments)
                add(take.speech, take.pause)
                if is_managed and not take.unmapped and len(segments) > start:
                    takes[name] = {"start": start, "end": len(segments)}
                add(take.after)
        for index, segment in enumerate(segments):
            segment["id"] = index
        transcript = {"segments": segments, "markers": markers, "duration": shifted_time(0, offset)}
        if managed:
            transcript["narration_takes"] = takes
        return chunks, transcript

    def state(self):
        return {"order": self.order, "leading": self.leading.state(),
                "takes": {name: take.state() for name, take in self.takes.items()}}

    @classmethod
    def from_state(cls, state, pcm):
        offset = 0

        def region(record):
            nonlocal offset
            length = count(record["frames"])
            clip = audio_slice(pcm, offset, offset + length)
            offset += length
            return Region(clip, record["segments"], record["markers"])

        def count(value):
            if type(value) is not int or value < 0 or offset + value > frames(pcm):
                raise ValueError("revision contains an invalid sample range")
            return value

        order = state["order"]
        takes = {}
        leading = region(state["leading"])
        if len(set(order)) != len(order) or not set(state["takes"]).issubset(order):
            raise ValueError("revision contains an invalid paragraph order")
        for name in order:
            if name in state["takes"]:
                record = dict(state["takes"][name])
                record["speech"] = region(record["speech"])
                offset += count(record["pause"])
                record["after"] = region(record["after"])
                takes[name] = ParagraphTake(**record)
        if offset != frames(pcm):
            raise ValueError("revision sample ranges do not cover the chapter")
        return cls(order, takes, leading)

    def reconcile(self, sentences, transfer_edited=True):
        """Transfer a retake's identity across an unambiguous one-paragraph edit."""
        current = [sentence.id for sentence in sentences]
        text = {sentence.id: normalized(sentence.text) for sentence in sentences}
        layout = []
        for tag, old_start, old_end, new_start, new_end in SequenceMatcher(
                a=self.order, b=current, autojunk=False).get_opcodes():
            old = self.order[old_start:old_end]
            new = current[new_start:new_end]
            if transfer_edited and tag == "replace" and len(old) == len(new) == 1 and old[0] in self.takes:
                previous = self.takes[old[0]]
                if (old[0] not in current and new[0] not in self.takes
                        and (not previous.unmapped or normalized(previous.text) == text[new[0]])):
                    previous.unmapped = False
                    self.takes[new[0]] = self.takes.pop(old[0])
                    if normalized(previous.text) == text[new[0]]:
                        previous.text = sentences[new_start].text
                    old = []
            # Keep audio for script removals until the user changes the chapter source.
            layout.extend(name for name in old if name not in current and name in self.takes)
            layout.extend(new)
        self.order = layout


def sidecar_words(raw):
    if isinstance(raw, list):
        return raw
    if isinstance(raw, dict):
        if isinstance(raw.get("words"), list) and raw["words"]:
            return raw["words"]
        if isinstance(raw.get("segments"), list):
            return [word for segment in raw["segments"] for word in segment_words(segment)]
    raise ValueError("timing sidecar requires a word array or word-timed segments")


def word_fields(word):
    if not isinstance(word, dict):
        raise ValueError("word boundary must be an object")
    adapted = {**word, "word": word.get("word", word.get("text")),
               "start": word.get("start", word.get("start_s")), "end": word.get("end", word.get("end_s"))}
    if "probability" in word and "confidence" not in word:
        adapted["confidence"] = word["probability"]
    return adapted


def segment_words(segment):
    if not isinstance(segment, dict) or not isinstance(segment.get("words"), list):
        raise ValueError("transcript segment must contain a word array")
    return segment["words"]


def read_segments(raw, pcm):
    if not isinstance(raw, dict) or not isinstance(raw.get("segments"), list) or not raw["segments"]:
        raise ValueError("existing chapter transcript requires timed segments")
    result = []
    previous = 0
    for entry in raw["segments"]:
        if not isinstance(entry, dict):
            raise ValueError("existing transcript segment must be an object")
        boundaries = segment_words(entry) if "words" in entry else []
        words = validate_words([word_fields(word) for word in boundaries], frames(pcm)) if boundaries else []
        start = seconds(entry.get("start", entry.get("start_s", words[0]["start"] if words else None)),
                        "segment start")
        word_end = max((word["end"] for word in words), default=start)
        end = seconds(entry.get("end", entry.get("end_s", word_end)), "segment end")
        if not previous <= start <= end or round(end * SAMPLE_RATE) > frames(pcm):
            raise ValueError("existing transcript segments must be ordered and within the audio")
        previous = start
        text = entry.get("text") or " ".join(word["word"] for word in words)
        if not isinstance(text, str):
            raise ValueError("existing transcript segment text must be a string")
        result.append({"text": text.strip(), "start": min(start, words[0]["start"] if words else start),
                       "end": max(end, word_end), "words": words})
    return result


def match_paragraphs(sentences, segments):
    """Match exact text first and infer one edited paragraph between known neighbors."""
    segment_text = [normalized(segment["text"]) for segment in segments]
    exact = []
    floor = 0
    for index, sentence in enumerate(sentences):
        target = normalized(sentence.text)
        matched = False
        for start in range(floor, len(segments)):
            prefix = ""
            for end in range(start, len(segments)):
                prefix += segment_text[end]
                if target and prefix == target:
                    exact.append((index, start, end + 1))
                    floor = end + 1
                    matched = True
                    break
                if not target.startswith(prefix):
                    break
            if matched:
                break
    partitions = []
    next_paragraph = next_segment = 0
    for index, start, end in [*exact, (len(sentences), len(segments), len(segments))]:
        if start > next_segment:
            gap = index - next_paragraph
            if gap == 1 and (exact or len(segments) == 1):
                partitions.append((next_paragraph, next_segment, start))
            elif gap == 0 and exact:
                partitions.append((None, next_segment, start))
            else:
                raise ValueError("cannot match existing speech to the chapter paragraphs; "
                                 "keep the original paragraph order and timed transcript")
        if index < len(sentences):
            partitions.append((index, start, end))
        next_paragraph, next_segment = index + 1, end
    return partitions


def match_tagged_paragraphs(sentences, segment_count, tags):
    """Keep managed paragraph identity even when a repeated take is missing."""
    if not isinstance(tags, dict):
        raise ValueError("narration_takes must map paragraph IDs to segment ranges")
    sentence_indexes = {sentence.id: index for index, sentence in enumerate(sentences)}
    ranges = []
    for name, span in tags.items():
        start = span.get("start") if isinstance(span, dict) else None
        end = span.get("end") if isinstance(span, dict) else None
        if type(start) is not int or type(end) is not int or not 0 <= start < end <= segment_count:
            raise ValueError("managed narration transcript contains an invalid segment range")
        ranges.append((start, end, sentence_indexes.get(name)))
    partitions = []
    previous_end = 0
    previous_index = -1
    for start, end, index in sorted(ranges, key=lambda entry: entry[0]):
        if start < previous_end or (index is not None and index <= previous_index):
            raise ValueError("managed narration transcript overlaps or differs from the current paragraph order")
        if start > previous_end:
            partitions.append((None, previous_end, start))
        partitions.append((index, start, end))
        previous_end = end
        if index is not None:
            previous_index = index
    if previous_end < segment_count:
        partitions.append((None, previous_end, segment_count))
    return partitions


def from_external(sentences, pcm, raw, legacy_ranges=None):
    segments = read_segments(raw, pcm)
    tags = raw.get("narration_takes")
    partitions = (match_tagged_paragraphs(sentences, len(segments), tags) if tags is not None
                  else match_paragraphs(sentences, segments))
    boundaries = []
    for index, first, limit in partitions:
        group = segments[first:limit]
        start = round(group[0]["start"] * SAMPLE_RATE)
        end = round(max(segment["end"] for segment in group) * SAMPLE_RATE)
        if legacy_ranges and limit == first + 1:
            start, end = legacy_ranges[first]
        if end <= start or (boundaries and start < boundaries[-1][2]):
            raise ValueError("existing transcript paragraph boundaries overlap or contain no audio")
        boundaries.append((index, start, end, group))

    order = [sentence.id for sentence in sentences]
    if not boundaries:
        raise ValueError("existing speech has no matching chapter paragraphs")
    leading_end = boundaries[0][1]
    leading = Region(audio_slice(pcm, 0, leading_end))
    regions = [(0, leading)]
    takes = {}
    for ordinal, (index, start, end, group) in enumerate(boundaries):
        gap_end = boundaries[ordinal + 1][1] if ordinal + 1 < len(boundaries) else frames(pcm)
        speech = Region(audio_slice(pcm, start, end), shifted_segments(group, -start))
        after = Region(audio_slice(pcm, end, gap_end))
        regions.extend(((start, speech), (end, after)))
        prior_text = " ".join(segment["text"] for segment in group)
        if index is None:
            next_id = next((sentences[i].id for i, *_ in boundaries[ordinal + 1:] if i is not None), None)
            name = f"unmapped-{ordinal}"
            while name in order:
                name = f"_{name}"
            order.insert(order.index(next_id) if next_id is not None else len(order), name)
        else:
            name = sentences[index].id
            if normalized(prior_text) == normalized(sentences[index].text):
                prior_text = sentences[index].text
        takes[name] = ParagraphTake(prior_text, speech, after, unmapped=index is None)
    markers = raw.get("markers", {})
    if not isinstance(markers, dict):
        raise ValueError("existing transcript markers must be an object with numeric times")
    for name, value in markers.items():
        time = seconds(value, f"marker '{name}'")
        frame = round(time * SAMPLE_RATE)
        if frame > frames(pcm):
            raise ValueError(f"marker '{name}' is outside the existing audio")
        offset, region = next((offset, region) for offset, region in reversed(regions) if offset <= frame)
        region.markers[name] = shifted_time(time, -offset)
    return Chapter(order, takes, leading)


def rebase_markers(previous, current):
    if previous is None:
        return {}
    old_text, old_times = character_times(previous.speech)
    new_text, new_times = character_times(current)
    matching = {}
    for old_start, new_start, length in SequenceMatcher(a=old_text, b=new_text, autojunk=False).get_matching_blocks():
        matching.update({old_start + index: new_start + index for index in range(length)})
    marks = {}
    for name, time in previous.speech.markers.items():
        if name in previous.authored_markers:
            continue
        # Use text positions so markers survive a provider that groups words differently.
        for index, (start, end) in enumerate(old_times):
            if start <= time < end or start == end == time:
                if index in matching:
                    new_start, new_end = new_times[matching[index]]
                    fraction = (time - start) / (end - start) if end > start else 0
                    time = new_start + fraction * (new_end - new_start)
                break
        marks[name] = round(min(max(0, time), frames(current.pcm) / SAMPLE_RATE), 6)
    return marks


def character_times(region):
    text = []
    times = []
    for segment in region.segments:
        for word in segment["words"]:
            value = normalized(word["word"])
            if value:
                text.append(value)
                start = word["start"]
                step = (word["end"] - start) / len(value)
                times.extend((start + index * step, start + (index + 1) * step) for index in range(len(value)))
    return "".join(text), times
