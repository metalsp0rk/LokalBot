# LokalBot autocomplete launch film

Use the [shared video instructions](../AGENTS.md). Resume from `BRIEF.md`, `frame.md`, `STORYBOARD.md`, `edit.json` and `narration.json`.

- `edit.json` is the edit: scene windows, footage source ranges, voice placements, caption chunks, overlay timing. `python3 build.py` writes `index.html`; `python3 mix_audio.py` writes `assets/sound/final-mix.wav`. Do not hand-edit `index.html`.
- Footage must be real screen recordings from the filming copy (`filming/`). Never redraw product UI or ghost text.
- Narration: `generate_breeze.py --runtime <dir with breeze-tts-2-q8_0.gguf and runtime/audiocpp_server>`; pick takes in `production/voice-plan.json`; `word_timings.py` refreshes caption timing evidence.
- Filming launches GUI apps and drives the keyboard: ask the user first, run the filming copy only while recording, and restore the real app's `lokalbotv3.cotypingStats` and `NSWindow Frame main` defaults afterwards.
- HyperFrames is pinned to 0.8.150 in `package.json`. `npm run check`, then render only after the user approves the preview.
