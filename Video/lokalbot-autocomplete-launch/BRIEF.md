# LokalBot autocomplete launch film

- Objective: the video at the top of the r/LocalLLaMA autocomplete post (draft kept local in `Docs/reddit-localllama-autocomplete-2026-10.md`, which is gitignored), positioned against Cotypist and Cotabby.
- User direction (2026-10-12): focus on autocomplete rather than meetings; compare with Cotypist and Cotabby; Breeze TTS voiceover; personal and warm, not a dry run-through of features.
- 2026-10-13 revision (user): remove the Serbian scene; show German, French, Italian and Spanish autocomplete at once on screen, animated nicely.
- Approved choices: Claude drives a filming-only build of LokalBot on the user's Mac; first-person narration as Stevan with real details (8,176 Cotypist Tabs from its stats file, English and Serbian every day, a Serbian message to family).
- Story: one person, one morning, a few notes. The hook is the Cotypist count. Then two wishes: that it remembers (a team update completed from yesterday's fictional meeting) and that it's open, so anyone can make it better in their own language (German, French, Italian and Spanish shown together). It closes with the credit to Cotabby and a request: tell me where it's wrong.
- Canvas: 1920 × 1080, 30 fps, about 60 seconds. Captions are burned in because Reddit autoplays muted.
- Footage: real screen recordings of typing in macOS apps with the filming build. The ghost text is real model output; nothing in the product UI is redrawn. Typing is driven by computer use, and the suggestions are whatever the model produced in that take.
- Filming build: `filming/build_filming_app.sh` archives master, changes only the ghost panel's `sharingType` from `.none` to `.readOnly` so screen recording can see it, and re-signs to the installed app's designated requirement so its Accessibility and Input Monitoring grants apply. It runs against `filming/seed_library.py`'s fictional library and an isolated settings suite with clipboard, screen memory, learning, capture and recording off.
- Voice: ElevenLabs eleven_v4, voice jBeSFLau956Zo30jykBV (user's choice, 2026-10-13; replaces the earlier Breeze TTS 2 narration). Disclosed as synthetic in the post.
- Look: warm paper background, real footage in soft-shadowed cards, small handwritten-style time stamps, gentle piano bed. No mock UI.
- Delivery: MP4 (H.264/AAC), SRT, poster; local only. Publishing needs separate approval.
- Licensing note: commercial use of the narration follows the ElevenLabs plan of the generating account.
