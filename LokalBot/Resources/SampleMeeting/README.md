# Sample meeting

"Sample: Orders API delivery window" is a made-up 3½-minute call between Maya, an API
product lead, and Daniel, a backend engineer. Both voices are synthetic. The first
line of the recording says so, and the people and the company are fictional.

**Try a sample meeting** is in the Meetings empty state and in Today's Get started
list on Mac, Windows and Linux. It downloads the models the sample needs, as their
own step and only when asked. It then imports this recording like any other file,
transcribes it, writes its notes, and suggests one question for Ask:

> Is the delivery window field required in the Orders API response?

The answer is no: the field is optional and left out when unknown. It should cite
the passage at 01:59–02:38 where the decision changes.

The meeting carries `"sampleID": "orders-api/en"` in `meta.json`. Ask, search, the
CLI and MCP see it, and their results mark it `"sample": true`. Day memory,
digests, the timeline, people, projects, action lists, About you, transcription
vocabulary, note exports and the GitHub star prompt leave it out. **Remove sample**
deletes it in one step. The Mac bundles this folder (`project.yml`). The Windows and
Linux app embeds the recording (`lokalbot_engine::sample`), and its CI source
archives include this folder.

## Files

| File | What it is |
| --- | --- |
| `orders-api.en.m4a` | The recording: one mono mixed track, AAC-LC at 40 kbit/s and 24 kHz, about 1 MB |
| `script.en.json` | The source script: turn ids, speakers, text, and the voices and model used |
| `reference.en.json` | Generated reference transcript, with each turn's measured start and end |
| `reference.en.md` | The same reference for a person to read |
| `expected.en.json` | The checklist: names, numbers, dates, decisions, the open question, the Ask question and the turns its answer should cite |

## What the meeting contains

| Item | Where |
| --- | --- |
| Names: Maya, Daniel, Priya, Tomás, Lena, Fernhill (a partner) | throughout |
| Numbers: 42 partners, 19 generated clients, p95 of 180 ms, mobile release 4.2, two million open orders | t10–t25 |
| Dates: November 18, then November 11, and Friday | t08, t19, t23 |
| A decision: Priya emails the three partners who asked; Daniel updates the API spec and changelog by Friday | t22–t23 |
| A decision that changes: first version 3, with the delivery window replacing ETA, by November 18. Later, no version 3: the response stays backward-compatible, ETA is unchanged, and the delivery window is a new optional field left out when unknown, shipping November 11 | t07–t09, then t18–t20 |
| An open question: whether to backfill delivery windows for older orders | t24–t28 |

## How it was made

`Scripts/sample_meeting.py build` voices each turn with ElevenLabs Eleven v4 (seed 7),
using the premade voices Bella (Maya) and Chris (Daniel). Each request passes the
neighbouring turns as context. Turn edges are trimmed and each turn is levelled to
the same loudness. Turns are joined with 0.45 s pauses within a speaker and 0.5–0.85 s
between speakers, with no overlap. The measured timings become the reference
transcript. Only the script text was sent to ElevenLabs, once, at build time. The
apps never contact a voice service.

Rebuilding needs `ELEVENLABS_API_KEY` (or `~/.config/elevenlabs/env`) and ffmpeg.
Synthesized turns are cached by text, voice and seed, so editing one line re-voices
only that line and its neighbours:

```sh
python3 Scripts/sample_meeting.py build --lang en
```

After a rebuild, keep `expected.en.json` and the `01:59–02:38` reference above in
step, and run `python3 -m unittest discover -s Scripts/tests -p test_sample_meeting.py`.

The generated audio is used under the ElevenLabs Terms of Service, and the script is
part of LokalBot (GPL-3.0). See `THIRD_PARTY_NOTICES.md`.

## Checking a transcript

```sh
python3 Scripts/sample_meeting.py check <meeting folder or transcript.json> [--summary summary.md]
```

It prints:

- the word error rate against the reference (character error rate for Chinese and Japanese)
- which names, numbers and dates came through
- how much speech time landed on the right speaker
- the checklist for a person: the notes state the final decision rather than the reversed one, the earlier decision is kept, the open question stays open, and Ask's answer cites the changed decision

Find the meeting folder with `lokalbot-cli path <id>`.

## Translations and the native-speaker test

German, Simplified Chinese and Serbian (Latin) come next. Each gets its own
`script.<lang>.json`, built audio, reference and `expected.<lang>.json`, plus a
`Sample` entry in `SampleMeeting.swift` and `lokalbot_engine::sample`. The app picks
the sample in the interface language and falls back to English.

A translation keeps:

- the turn ids, speakers and order
- the names
- every fact in the checklist, with numbers and dates said the way a native speaker says them

`delivery window`, `ETA` and `Orders API` stay as identifiers. Choose voices that
speak the language natively, and record them in the script.

A native speaker then:

1. switches the interface language
2. runs **Try a sample meeting**
3. runs `check` on the result
4. goes through the printed checklist, reading the notes and the Ask answer in their own language

## The coding-agent demo

The changed decision is technical on purpose. In a repository with an `OrderResponse`
type, ask a coding agent to add delivery windows and to check LokalBot first. Either
of these finds the passage:

- `lokalbot-cli search "delivery window"`
- MCP `search_meetings`

`lokalbot-cli get <id> --include summary` then gives the decision: keep the response
backward-compatible, keep `ETA`, and add `delivery_window` as an optional field
omitted when unknown.
