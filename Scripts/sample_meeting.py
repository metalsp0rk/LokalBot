#!/usr/bin/env python3
"""Build and check the bundled sample meeting (LokalBot/Resources/SampleMeeting).

    sample_meeting.py build [--lang en] [--cache DIR]
        Voice every turn of script.<lang>.json with ElevenLabs (ELEVENLABS_API_KEY,
        or ~/.config/elevenlabs/env), join the turns into one mono recording,
        and write <audio>.m4a plus the reference transcript (reference.<lang>.json
        and reference.<lang>.md) with each turn's measured start and end.
        Only the script text leaves the computer, and only here: the apps never
        call a voice service. Synthesized turns are cached by text, voice, model
        and seed, so editing one line re-voices only that line.

    sample_meeting.py check [--lang en] <transcript.json | meeting folder> [--summary FILE]
        Score a LokalBot transcript of the sample against the reference: word
        error rate (character error rate for Chinese and Japanese), which names,
        numbers and dates came through, and how well speakers were separated.
        Then print the checklist a person goes through for the notes and Ask.

Needs ffmpeg and ffprobe on PATH for build; check uses only the standard library.
"""
import argparse, hashlib, json, os, re, struct, subprocess, sys, tempfile, unicodedata, urllib.request, wave

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SAMPLE_DIR = os.path.join(ROOT, "LokalBot", "Resources", "SampleMeeting")
RATE = 24_000
LEAD_IN = 0.6
TAIL = 1.2
TARGET_RMS_DB = -21.0


def load(lang, kind):
    with open(os.path.join(SAMPLE_DIR, f"{kind}.{lang}.json"), encoding="utf-8") as f:
        return json.load(f)


def stamp(seconds):
    s = int(seconds)
    return f"{s // 60:02d}:{s % 60:02d}"


# ---------------------------------------------------------------- build

def api_key():
    key = os.environ.get("ELEVENLABS_API_KEY")
    if key:
        return key
    path = os.path.expanduser("~/.config/elevenlabs/env")
    if os.path.exists(path):
        for line in open(path, encoding="utf-8"):
            name, _, value = line.strip().partition("=")
            if name.removeprefix("export ").strip() == "ELEVENLABS_API_KEY":
                return value.strip().strip("'\"")
    sys.exit("Set ELEVENLABS_API_KEY to voice the script.")


def synthesize(text, voice, tts, previous, following, cache):
    """One turn as 24 kHz mono 16-bit PCM, edges trimmed. Cached by content."""
    ident = json.dumps([text, voice, tts["model"], tts["seed"], previous, following], ensure_ascii=False)
    digest = hashlib.sha256(ident.encode()).hexdigest()[:20]
    wav_path = os.path.join(cache, f"{digest}.wav")
    if not os.path.exists(wav_path):
        body = {"text": text, "model_id": tts["model"], "seed": tts["seed"]}
        if previous:
            body["previous_text"] = previous
        if following:
            body["next_text"] = following
        request = urllib.request.Request(
            f"https://api.elevenlabs.io/v1/text-to-speech/{voice}?output_format=mp3_44100_128",
            data=json.dumps(body).encode(),
            headers={"xi-api-key": api_key(), "Content-Type": "application/json"})
        mp3_path = os.path.join(cache, f"{digest}.mp3")
        with urllib.request.urlopen(request, timeout=180) as response:
            with open(mp3_path, "wb") as f:
                f.write(response.read())
        trim = ("silenceremove=start_periods=1:start_threshold=-50dB,areverse,"
                "silenceremove=start_periods=1:start_threshold=-50dB,areverse")
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", mp3_path, "-af", trim,
                        "-ar", str(RATE), "-ac", "1", "-c:a", "pcm_s16le", wav_path + ".tmp.wav"], check=True)
        os.replace(wav_path + ".tmp.wav", wav_path)
    with wave.open(wav_path) as reader:
        assert (reader.getnchannels(), reader.getsampwidth(), reader.getframerate()) == (1, 2, RATE)
        return reader.readframes(reader.getnframes())


def level(pcm):
    """Scale a turn to the shared speech level, so both voices sit alike."""
    count = len(pcm) // 2
    samples = struct.unpack(f"<{count}h", pcm)
    rms = (sum(s * s for s in samples) / max(count, 1)) ** 0.5
    if rms == 0:
        raise RuntimeError("a synthesized turn is silent")
    gain = (32768 * 10 ** (TARGET_RMS_DB / 20)) / rms
    gain = min(gain, 32000 / max(abs(s) for s in samples))
    return struct.pack(f"<{count}h", *(max(-32768, min(32767, int(s * gain))) for s in samples))


def gap(turns, index):
    """Pause before a turn: shorter when the same person keeps talking, a little
    longer before a change of subject. Deterministic, so rebuilds line up."""
    turn = turns[index]
    if "gapBefore" in turn:
        return turn["gapBefore"]
    if turns[index - 1]["speaker"] == turn["speaker"]:
        return 0.45
    jitter = int(hashlib.sha256(turn["id"].encode()).hexdigest()[:4], 16) / 0xFFFF
    return round(0.5 + 0.35 * jitter, 2)


def build(args):
    script = load(args.lang, "script")
    turns, speakers, tts = script["turns"], script["speakers"], script["tts"]
    cache = args.cache or os.path.join(tempfile.gettempdir(), "lokalbot-sample-meeting")
    os.makedirs(cache, exist_ok=True)
    mix = bytearray(bytes(int(LEAD_IN * RATE) * 2))
    reference = []
    for index, turn in enumerate(turns):
        if index:
            mix.extend(bytes(int(gap(turns, index) * RATE) * 2))
        previous = turns[index - 1]["text"] if index else None
        following = turns[index + 1]["text"] if index + 1 < len(turns) else None
        pcm = level(synthesize(turn["text"], speakers[turn["speaker"]]["voice"], tts, previous, following, cache))
        start = len(mix) / 2 / RATE
        mix.extend(pcm)
        end = len(mix) / 2 / RATE
        reference.append({"id": turn["id"], "speaker": turn["speaker"], "name": speakers[turn["speaker"]]["name"],
                          "start": round(start, 2), "end": round(end, 2), "text": turn["text"]})
        print(f"{turn['id']} {stamp(start)} {end - start:5.2f}s {speakers[turn['speaker']]['name']}: {turn['text'][:60]}")
    mix.extend(bytes(int(TAIL * RATE) * 2))

    with tempfile.TemporaryDirectory() as work:
        mix_path = os.path.join(work, "mix.wav")
        with wave.open(mix_path, "wb") as writer:
            writer.setnchannels(1)
            writer.setsampwidth(2)
            writer.setframerate(RATE)
            writer.writeframes(bytes(mix))
        out = os.path.join(SAMPLE_DIR, script["audio"])
        # AAC-LC: symphonia (Linux, Windows) and AVFoundation (Mac) both decode it.
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", mix_path, "-c:a", "aac", "-b:a", "40k",
                        "-ar", str(RATE), "-ac", "1", "-map_metadata", "-1", "-movflags", "+faststart",
                        "-metadata", f"title={script['title']}",
                        "-metadata", "comment=LokalBot sample meeting. Synthetic voices; the people and company are fictional.",
                        "-fflags", "+bitexact", "-flags:a", "+bitexact", out], check=True)
    duration = len(mix) / 2 / RATE
    with open(os.path.join(SAMPLE_DIR, f"reference.{args.lang}.json"), "w", encoding="utf-8") as f:
        json.dump({"sampleID": script["sampleID"], "language": args.lang, "title": script["title"],
                   "duration": round(duration, 2), "turns": reference}, f, ensure_ascii=False, indent=2)
        f.write("\n")
    with open(os.path.join(SAMPLE_DIR, f"reference.{args.lang}.md"), "w", encoding="utf-8") as f:
        f.write(f"# {script['title']}\n\n")
        f.write("Reference transcript of the LokalBot sample meeting: what each synthetic voice was asked to say, "
                "with the time each turn starts in the recording. The people and the company are fictional.\n\n")
        for turn in reference:
            f.write(f"**[{stamp(turn['start'])}] {turn['name']}:** {turn['text']}\n\n")
    size = os.path.getsize(out)
    print(f"\n{out}: {stamp(duration)} ({duration:.1f} s), {size / 1e6:.2f} MB")


# ---------------------------------------------------------------- check

ONES = "zero one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen".split()
TENS = "_ _ twenty thirty forty fifty sixty seventy eighty ninety".split()
ORDINALS = {"one": "first", "two": "second", "three": "third", "five": "fifth", "eight": "eighth",
            "nine": "ninth", "twelve": "twelfth"}


def words_for(n):
    if n < 20:
        return ONES[n]
    if n < 100:
        return TENS[n // 10] + ("" if n % 10 == 0 else " " + ONES[n % 10])
    for size, name in ((1_000_000_000, "billion"), (1_000_000, "million"), (1000, "thousand"), (100, "hundred")):
        if n >= size:
            rest = n % size
            return words_for(n // size) + " " + name + ("" if rest == 0 else " " + words_for(rest))
    return str(n)


def ordinal(words):
    head, _, last = words.rpartition(" ")
    if last in ORDINALS:
        last = ORDINALS[last]
    elif last.endswith("y"):
        last = last[:-1] + "ieth"
    else:
        last += "th"
    return (head + " " + last).strip()


def spell_numbers(text):
    text = re.sub(r"(?<=\d),(?=\d{3})", "", text)
    text = re.sub(r"(\d+)(st|nd|rd|th)\b", lambda m: ordinal(words_for(int(m.group(1)))), text)
    text = re.sub(r"(\d+)\.(\d+)",
                  lambda m: words_for(int(m.group(1))) + " point " + " ".join(ONES[int(d)] for d in m.group(2)), text)
    text = re.sub(r"(?<=[a-z])(?=\d)|(?<=\d)(?=[a-z])", " ", text)
    return re.sub(r"\d+", lambda m: words_for(int(m.group(0))), text)


FILLERS = {"um", "uh", "hm", "hmm", "mm", "ah", "er"}


def normalize(text, lang):
    text = unicodedata.normalize("NFKC", text).lower()
    if lang == "en":
        text = spell_numbers(text.replace("-", " "))
        text = re.sub(r"\ba hundred\b", "one hundred", text)
        text = re.sub(r"\bokay\b", "ok", text)
    text = re.sub(r"[^\w\s']|_", " ", text).replace("'", "")
    tokens = [t for t in text.split() if t not in FILLERS]
    if lang == "en":  # "a hundred and eighty" and "180" should agree
        tokens = [t for i, t in enumerate(tokens)
                  if not (t == "and" and 0 < i < len(tokens) - 1 and tokens[i - 1] == "hundred")]
    if lang in ("zh", "ja"):
        return [c for c in "".join(tokens)]
    return tokens


def edit_distance(ref, hyp):
    row = list(range(len(hyp) + 1))
    for i, r in enumerate(ref, 1):
        prev, row[0] = row[0], i
        for j, h in enumerate(hyp, 1):
            prev, row[j] = row[j], min(row[j] + 1, row[j - 1] + 1, prev + (r != h))
    return row[-1]


def speaker_accuracy(reference, segments):
    """Share of transcribed speech time whose speaker label maps to the right
    voice, after giving each label the voice it overlaps most."""
    overlap = {}
    for seg in segments:
        for turn in reference:
            shared = min(seg["end"], turn["end"]) - max(seg["start"], turn["start"])
            if shared > 0:
                key = (seg.get("speaker", "?"), turn["speaker"])
                overlap[key] = overlap.get(key, 0) + shared
    total = sum(overlap.values())
    if not total:
        return 0.0, {}
    mapping, used = {}, set()
    for (label, voice), _ in sorted(overlap.items(), key=lambda item: -item[1]):
        if label not in mapping and voice not in used:
            mapping[label] = voice
            used.add(voice)
    matched = sum(t for (label, voice), t in overlap.items() if mapping.get(label) == voice)
    return matched / total, mapping


def check(args):
    reference = load(args.lang, "reference")
    expected = load(args.lang, "expected")
    path = args.transcript
    if os.path.isdir(path):
        path = os.path.join(path, "transcript.json")
    with open(path, encoding="utf-8") as f:
        segments = json.load(f)["segments"]
    hyp_text = " ".join(s["text"] for s in segments)
    ref_text = " ".join(t["text"] for t in reference["turns"])
    ref_tokens, hyp_tokens = normalize(ref_text, args.lang), normalize(hyp_text, args.lang)
    errors = edit_distance(ref_tokens, hyp_tokens)
    unit = "CER" if args.lang in ("zh", "ja") else "WER"
    print(f"{unit}: {errors / len(ref_tokens):.1%} ({errors} edits over {len(ref_tokens)} reference "
          f"{'characters' if unit == 'CER' else 'words'})")

    flat = " " + " ".join(normalize(hyp_text, "raw")) + " "
    raw = unicodedata.normalize("NFKC", hyp_text).lower()
    found = 0
    print("\nNames, numbers and dates:")
    for entity in expected["entities"]:
        hit = any(v in raw or f" {' '.join(normalize(v, 'raw'))} " in flat for v in entity["any"])
        found += hit
        print(f"  {'✓' if hit else '✗'} {entity['kind']:<6} {entity['label']}")
    print(f"  {found}/{len(expected['entities'])} came through")

    labels = sorted({s.get("speaker", "?") for s in segments})
    accuracy, mapping = speaker_accuracy(reference["turns"], segments)
    print(f"\nSpeakers: {len(labels)} label(s) for {expected['speakers']} voices "
          f"({', '.join(f'{label} → {mapping.get(label, 'none')}' for label in labels)}); "
          f"{accuracy:.0%} of speech time on the right voice")

    print("\nFor a person to check:")
    print(f"  [ ] The notes state the final decision, not the reversed one:\n      {expected['changedDecision']['after']}")
    print(f"  [ ] The notes keep this decision:\n      {expected['decision']['text']}")
    print(f"  [ ] The open question stays open:\n      {expected['openQuestion']['text']}")
    ask = expected["ask"]
    cited = [t for t in reference["turns"] if t["id"] in ask["citesTurns"]]
    when = f"{stamp(cited[0]['start'])}–{stamp(cited[-1]['end'])}" if cited else "?"
    print(f"  [ ] Ask \"{ask['question']}\" answers:")
    for point in ask["answerPoints"]:
        print(f"        - {point}")
    print(f"      and cites the passage at {when}.")
    if args.summary:
        print("\nNotes under review:\n")
        print(open(args.summary, encoding="utf-8").read())


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    b = sub.add_parser("build")
    b.add_argument("--lang", default="en")
    b.add_argument("--cache")
    c = sub.add_parser("check")
    c.add_argument("--lang", default="en")
    c.add_argument("transcript")
    c.add_argument("--summary")
    args = parser.parse_args()
    build(args) if args.command == "build" else check(args)


if __name__ == "__main__":
    main()
