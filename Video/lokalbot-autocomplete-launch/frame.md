# Frame spec — "Since May"

Concept: a quiet morning at the desk. Real TextEdit pages sit on warm paper like letters on a table, and the ghost text is the only thing that moves without a hand. The film is one person talking about something he made, not a feature tour.

## Palette

| Role | Hex | Use |
|---|---|---|
| Paper (background) | `#F3EDE3` | Every scene. Fine grain on top at 10% so it never reads as a blank slide. |
| Paper shade | `#E8DFD1` | Card shadows' warm floor, the hairline rules. |
| Ink | `#24211D` | Captions, numbers, titles. |
| Ink soft | `#5B544B` | Labels, time stamps. Must still pass AA on paper. |
| Accent (LokalBot teal, deepened for paper) | `#16786C` | One focal hit per scene: the "from yesterday's call" tag, the GitHub line, the counter's underline. |

No other hues. The footage is TextEdit's own white page (#FDFDFD) with black type and the gray ghost.

## Type

- Newsreader (variable, OFL) for voice: captions at 46 px, 400 weight, italic for the time stamps. The big number on the hook at 220 px, 500 weight.
- Hanken Grotesk (variable, OFL) for labels at 22–26 px, 600 weight, letter-spacing 0.06em, uppercase sparingly.
- DM Mono only for the GitHub URL on the close.

## Layout

- Footage card: TextEdit's text area cropped below the ruler, on a #FDFDFD card with 14 px radius, `0 40px 80px -30px rgba(60,40,20,.45)` shadow, centered slightly above middle so captions sit below it on paper. Never full-bleed: the paper around it is the warmth.
- Captions: bottom band, left-aligned to the card's left edge, max two lines, 1100 px wide.
- Label: top-left of the card, italic Newsreader 28 px, naming who the note is for ("To the team", "Za mamu · to Mom").
- Camera: a slow push of 4–5%, anchored at the left edge so the start of each line never leaves the card.

## Motion

Soft and unhurried: power2/sine eases, 0.5–0.9 s entrances, cross-dissolves of 0.6 s between scenes. Ambient: the grain shifts subtly; a faint warm vignette breathes. No bounces, no glitch, no whooshes.

## Don'ts

- No mock UI, no redrawn ghost text, no fake cursor. Every suggestion on screen is real model output from the take.
- No feature bullet lists on screen. The narration carries meaning; the screen carries proof.
- No pure black or white; no gradients behind text.
