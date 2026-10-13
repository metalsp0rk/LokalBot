# Storyboard — "Since May" (62.8 s, 1920 × 1080)

One person, one evening, a few messages. Real typing in TextEdit with the filming copy of LokalBot; the gray ghost is real model output. Narration is first person (Breeze TTS 2, male reference, warm direction). Times are global seconds from `edit.json`.

## Frame 1 — hook · 0.0–11.8
status: outline · src: assets/footage/hook.mp4 · rules: card-on-paper entrance, slow camera push, stat count-in (counter rule draws left→right)
- Footage: an English reply being typed; a ghost appears; Tab, Tab, Tab, word by word.
- Overlay: "8,176", a teal rule, "TABS ACCEPTED IN COTYPIST SINCE MAY 28", and "counted by its own stats file".
- VO 01 "Since May, I've pressed Tab in Cotypist 8,176 times." VO 02 "It changed how I write. But I wanted two more things."

## Frame 2 — remember · 11.8–25.6
status: outline · src: assets/footage/remember.mp4 · rules: camera push toward the caret, pill tag pop, note card slide-in with a slight rotation
- Footage: "Hi all, / Quick update after my call with Mila: the Harbor beta" → during the pause the ghost offers "ships Thursday morning, after" → " ships " typed → Tabs → "Thursday morning, after the accessibility review."
- Overlay: teal pill "from yesterday's call" beside the ghost; then the meeting-note card quoting the seeded summary line.
- VO 03 "I wanted it to remember…" VO 04 "That comes from my own meeting notes, and it never leaves my Mac."

## Frame 3 — languages · 25.6–38.6
status: outline · src: assets/footage/lang-{de,fr,it,es}.mp4 · rules: staggered row entrance (slide from right with a slight rotation), chip highlight synced to the spoken language name, joint exit
- Four rows stacked on paper, each a real TextEdit take: Deutsch, Français, Italiano, Español. Rows enter 0.45 s apart; each chip turns teal as its language is named in the narration (31.9, 32.8, 33.4, 34.3 s).
- The takes were typed one after another and are shown together; none is sped up beyond the stated rate.
- VO 05 "And I wanted it open, so anyone can make it better in their own language. Here it's writing German, French, Italian and Spanish. Same small model, on your Mac."

## Frame 4 — quiet · 38.6–47.0
status: outline · src: assets/footage/quiet.mp4 · rules: hold, no push during the quiet beat
- Footage: "Hi Mila," then a pause with no ghost. Then "…great to finally " and Tabs take "meet you in person."
- VO 06 "When it isn't sure, it stays quiet. When it is, it offers a few words. Tab takes one at a time."

## Frame 5 — credit and close · 47.0–62.8
status: outline · rules: end card with the brand mark and staggered lines, then the URL reveal
- LokalBot mark and wordmark. "Free and open source. Every model runs on your Mac." "Autocomplete built on Cotabby's open loop, with Cotypist as the bar for how it should feel." At 57.1 s: github.com/stevyhacker/lokalbot.
- VO 07 credit line; VO 08 "Try it, and tell me where it's wrong."

## Filmed takes (2026-10-13, TextEdit 40 pt Helvetica, window centered on the LG at 1x)

Raw full-display recordings are kept outside the repository (they show the whole screen); `filming/crop_takes.sh` crops the text area (1692×286 at x 860, y 520) into `assets/footage/`. The Serbian take (family-2) was cut at the user's request on 2026-10-13. Takes from the first pass (hook-1, remember-1/2, family-1, quiet-1) and two quiet Italian attempts are not used: a floating Claude window and the filming copy's own LokalBot window covered parts of TextEdit, and the computer-use edge glow tinted the left margin.

| Scene | Take | Typed | Accepted with Tab (real model output) | Source range used |
|---|---|---|---|---|
| hook | hook-2 | "Hey, thanks for the help with the release notes. I'll send " | "you a message with the details." | 7.4–19.2 s |
| remember | remember-3 | "Hi all, / Quick update after my call with Mila: the Harbor beta" (pause) " ships " | ghost "ships Thursday morning, after" appears during the pause; Tabs give "Thursday morning, after the accessibility review." | 6.0–19.8 s |
| languages · DE | lang-de | "Hallo Anna, ich komme morgen etwas später, weil mein Zug " | "verspätet ist." | from 5.3 s |
| languages · FR | lang-fr | "Bonjour Claire, merci pour hier, c'était un plaisir de te " | "rencontrer." | from 4.7 s |
| languages · IT | lang-it-3 | "Ciao Marco, arrivo domani mattina. Ti chiamo appena " (ghost "per telefono." showed before "appena") | "arrivo." | from 4.5 s |
| languages · ES | lang-es | "Hola Lucía, ¡feliz cumpleaños! Te deseo un " | "día lleno de alegría", ghost ", risas y momentos" | 4.3–7.25 s, then 12.0–20.4 s |
| quiet | quiet-2 | "Hi Mila," (3.4 s, no suggestion) then "Thanks for yesterday, it was great to finally " | "meet you in person." | 5.2–14.7 s at 1.25× |

Line 06 of the narration was rewritten to match the quiet take's order (silence first, then words).
