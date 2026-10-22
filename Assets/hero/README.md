# README hero

The root [README](../../README.md) opens with [lokalbot-demo-light.gif](lokalbot-demo-light.gif)
or [lokalbot-demo-dark.gif](lokalbot-demo-dark.gif), whichever matches the
viewer's appearance. Both are 14-second loops of the real app in three scenes:
a Quick Recall search for "pricing" that finds screen text and a meeting, a
decision in the meeting summary that opens the transcript at its source line,
and an Ask answer whose evidence lists its sources.

Each scene is a recording of the LokalBot UI Test Host window driven by real
keyboard and pointer events; the window's pixels are unchanged apart from
cropping and scaling. The caption above each scene, the footer, and the ring
drawn where a click landed were added afterwards. The library is the fictional
one in `Scripts/readme-gif/seed_library.py`. The isolated host suppresses
capture and inference, so the transcript, summary, decisions and the Ask answer
are fixtures; the search results are the app's own search over that library.

The recordings were made on **October 22, 2026** from the `v0.11.2` release
commit [`749987d`](https://github.com/stevyhacker/lokalbot/tree/749987d60be964dcbf154707e225ae3fcc95d0cf),
built as the UI Test Host with `CODE_SIGNING_ALLOWED=NO`, with a temporary
library and UserDefaults suite for every take. The installed app and user
library were not used, and no UI tests were run.
[Scripts/readme-gif/README.md](../../Scripts/readme-gif/README.md) explains how
to record and rebuild them.

| File | Dimensions | Length | Bytes | SHA-256 |
| --- | --- | --- | --- | --- |
| `lokalbot-demo-light.gif` | 1840 × 1344 | 14 s at 20 fps | 1,355,517 | `ce5db8cb3435c0d37b480ca4889ef637ca904469c6d5c94095555f03b9dbed2d` |
| `lokalbot-demo-dark.gif` | 1840 × 1344 | 14 s at 20 fps | 1,111,335 | `b57f74bebf7e0b295b89ddd1b1f53a59147818cae76fb0dae1df62e3e20b6d75` |

## Still composite

The earlier README hero, [lokalbot-hero-light.png](lokalbot-hero-light.png)
and [lokalbot-hero-dark.png](lokalbot-hero-dark.png), stays for the social
preview card (`Assets/social-preview/social-preview.html`). Each is a composite:
two unedited captures of the native SwiftUI views on a drawn background, under
the tagline, with "saw" and "said"
labels beside Quick Recall's Screens and Meetings groups. Nothing inside the
captures was retouched. All content is fictional demo data from
`Scripts/seed_demo_library.py --profile studio`. Both images pair the holiday
shoot planning call with a search on a different topic, "demo presentation";
the headline already says "Mac", so the search avoids it.

All four captures came from one seeded library on **October 9, 2026**, so the
times they show agree. The LokalBot UI Test Host was built from the `v0.10.2`
release commit
[`1ade074`](https://github.com/stevyhacker/lokalbot/tree/1ade074a90510866d8909dc17e2e1dde372831b7)
(build 46) and ran with a temporary storage root and UserDefaults suite. The
installed app and user library were not used, and no UI tests were run.

| Capture | Settings |
| --- | --- |
| Meeting window | `LOKALBOT_INITIAL_SECTION=meetings LOKALBOT_SELECT_INDEX=0 LOKALBOT_DETAIL_TAB=summary`, `LOKALBOT_CAPTURE_SIZE=1400x880` at 2× |
| Quick Recall | `LOKALBOT_UI_TEST_WINDOW=quick-recall LOKALBOT_QUICK_RECALL_QUERY="demo presentation"`, `LOKALBOT_CAPTURE_SIZE=660x512` with `LOKALBOT_CAPTURE_SCALE=4` |

Each was captured with `LOKALBOT_CAPTURE_APPEARANCE=light` and `dark`, plus
`LOKALBOT_DISMISS_ONBOARDING=1` and the rest of the environment from
`capture()` in `Scripts/capture-screenshots.sh`.

Compose a hero from two captures (requires Google Chrome):

```bash
python3 Scripts/render_readme_hero.py --theme light --meetings meetings.png --recall quick-recall.png --out Assets/hero/lokalbot-hero-light.png
```

Both published files are that script's output for the captures above.

| File | Dimensions | Bytes | SHA-256 |
| --- | --- | --- | --- |
| `lokalbot-hero-light.png` | 3200 × 1800 | 3,926,159 | `9505bfdf22359ab1503a353bc8b5d82e93b65c71814854d7f579e2c3b073c465` |
| `lokalbot-hero-dark.png` | 3200 × 1800 | 2,905,569 | `3b7c554d264c66bea2831b7e56b5bffb85f0cb9f2b33ed1c75d1e0b59ae4e5c3` |
