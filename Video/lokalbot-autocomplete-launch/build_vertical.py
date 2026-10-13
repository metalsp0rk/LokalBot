"""Build the 9:16 cut (1080 × 1920, X and TikTok) from the 30-second cut.

Same story, voice, captions, timings and audio as short/; only the layout
changes. Footage is shown at 1.2× (about 48 px text) in a 900 px card, clear
of TikTok's right-hand icons, and the camera pans along each line so the caret
and the ghost stay in view. Pans come from short/production/pan-tracks.json
(see pan_track.py), so they follow what was actually typed, and each stop lands
on a word start. Writes vertical/index.html and copies the assets it needs.

Usage: python3 build_vertical.py
"""
import html
import json
import shutil
import subprocess
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent
SRC = ROOT / 'short'
OUT = ROOT / 'vertical'
E = json.loads((SRC / 'edit.json').read_text())
TRACKS = json.loads((SRC / 'production/pan-tracks.json').read_text())
TOTAL = E['duration']

S = 1.2                    # footage scale
CARD_X, CARD_W, PAD_X, PAD_Y = 60, 900, 24, 20   # clear of TikTok's right-hand icons
WIN = (CARD_W - 2 * PAD_X) / S   # visible source width
PITCH = 48

# Copy assets the vertical page needs: fonts, brand, and only the clips it plays.
for sub in ('assets/fonts', 'assets/brand'):
    shutil.copytree(SRC / sub, OUT / sub, dirs_exist_ok=True)
used = {s['footage'] for s in E['scenes'] if 'footage' in s}
used |= {r['footage'] for s in E['scenes'] if s.get('type') == 'rows' for r in s['rows']}
if (OUT / 'assets/footage').exists():
    shutil.rmtree(OUT / 'assets/footage')
(OUT / 'assets/footage').mkdir(parents=True)
for rel in sorted(used):
    shutil.copy2(SRC / rel, OUT / rel)
(OUT / 'assets/sound').mkdir(parents=True, exist_ok=True)
shutil.copy2(SRC / 'assets/sound/final-mix.wav', OUT / 'assets/sound/final-mix.wav')
mix_dur = round(float(subprocess.run(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0',
                                      str(OUT / 'assets/sound/final-mix.wav')], check=True, capture_output=True,
                                     text=True).stdout), 3)


def card_height(lines):
    return round(PAD_Y + (lines * PITCH + 10) * S + 14)


def frame_at(clip, t):
    raw = subprocess.run(['ffmpeg', '-v', 'error', '-ss', f'{t:.3f}', '-i', str(SRC / 'assets/footage' / clip),
                          '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'gray', '-'],
                         check=True, capture_output=True).stdout
    return np.frombuffer(raw, dtype=np.uint8).reshape(286, 1692)


def word_starts(clip, t, line):
    """Where words start on one text line of the frame at media time t, less a
    5 px lead. Inter-word gaps at 40 pt are 11-18 px; gaps inside words are at
    most 6 px."""
    y0, y1 = TRACKS[clip]['bands'][line]
    ink = (frame_at(clip, t)[y0:y1 + 1, :1692 - 24] < 235).any(axis=0)
    return [c - 5 for c in range(10, len(ink)) if ink[c] and not ink[c - 10:c].any()]


def trace(clip, segments):
    """(global time, media time, caret line, caret x, ink end) every 0.1 s."""
    out = []
    for seg in segments:
        rate = seg.get('rate', 1.0)
        m0 = seg['media_start']
        m1 = m0 + (seg['end'] - seg['start']) * rate
        for t, row in TRACKS[clip]['samples']:
            if m0 - 1e-6 <= t <= m1 + 1e-6:
                line = max((i for i, c in enumerate(row) if c[0] >= 0), default=None)
                caret, ink = row[line] if line is not None else (None, None)
                out.append((seg['start'] + (t - m0) / rate, t, line, caret, ink))
    return out


def pan_keys(clip, segments):
    """Camera stops (global time, source px) for one clip shown in segments.

    A stop is placed from what happens during its first 0.6 s: the smallest
    offset that starts on a word and keeps every typed and ghost pixel on
    screen, clear of the right fade, while leaving at least 80 px of text
    before the caret. If a long ghost makes both impossible, the stop keeps
    the caret's context and lets the ghost run into the fade. A stop holds
    until a frame no longer fits: ink past the right edge, or the caret too
    close to the left (a new line). Frames with no caret (blink, empty field)
    never move it.
    """
    # A word start up to SLACK px left of the ideal is fine: the last ink then
    # sits just inside the fade, still nearly opaque.
    SLACK = 14

    def place(stop):
        lo, hi = stop['lo'], stop['hi']
        if lo <= SLACK:
            return 0
        starts = [0] + word_starts(clip, stop['t'], stop['line'])
        if lo <= hi:
            fit = [c for c in starts if lo - SLACK <= c <= hi]
            x = fit[0] if fit else lo
        else:
            x = max(c for c in starts if c <= max(hi, 0))
        return max(0, round(x))

    stops, open_ = [], None
    for g, t, line, caret, ink in trace(clip, segments):
        if line is None:
            continue
        lo, hi = max(caret, ink) + 34 - WIN, caret - 80
        if open_ is None:
            open_ = {'g': segments[0]['start'], 'lo': lo, 'hi': hi, 'line': line, 't': t, 'x': None}
            continue
        if open_['x'] is None:
            if g - open_['g'] < 0.6:
                open_['lo'], open_['hi'] = max(open_['lo'], lo), min(open_['hi'], hi)
                open_['line'], open_['t'] = line, t   # word starts come from where the caret ends up
                continue
            open_['x'] = place(open_)
            stops.append((round(open_['g'], 3), open_['x']))
        x = open_['x']
        if lo - SLACK <= x and (hi >= x or x == 0):
            continue
        open_ = {'g': g, 'lo': lo, 'hi': hi, 'line': line, 't': t, 'x': None}
    if open_ is not None and open_['x'] is None:
        stops.append((round(open_['g'], 3), place(open_)))
    if not stops:
        return [(round(segments[0]['start'], 3), 0)]
    keys = [stops[0]]
    for g, x in stops[1:]:
        if x != keys[-1][1]:
            keys.append((g, x))
    return keys


cards, rows_html, pans = [], [], []
layout = {
    'hook': {'y': 600, 'lines': 1},
    'remember': {'y': 540, 'lines': 4},
    'quiet': {'y': 520, 'lines': 3},
}
for s in E['scenes']:
    if 'footage' in s:
        lay = layout[s['id']]
        h = card_height(lay['lines'])
        clip = Path(s['footage']).name
        dur = round(s['end'] - s['start'], 3)
        rate = f' data-playback-rate="{s["rate"]}"' if s.get('rate') else ''
        cards.append(f'''
    <div class="card-wrap" id="card-{s['id']}" style="left:{CARD_X}px;top:{lay['y']}px;width:{CARD_W}px;height:{h}px">
      <div class="card-mask"><div class="card-cam" id="cam-{s['id']}" data-layout-allow-overflow>
        <video id="v-{s['id']}" class="clip footage" src="{s['footage']}" muted playsinline
          style="left:0;top:{PAD_Y}px;width:{round(1692 * S)}px;height:{round(286 * S)}px"
          data-start="{s['start']}" data-duration="{dur}" data-media-start="{s['media_start']}"
          data-track-index="{1 + len(cards)}"{rate}></video>
      </div></div>
    </div>
    <div class="stamp" id="stamp-{s['id']}" style="left:{CARD_X + 6}px;top:{lay['y'] - 54}px">{html.escape(s['stamp'])}</div>''')
        pans.append({'cam': f"#cam-{s['id']}", 'keys': pan_keys(clip, [
            {'start': s['start'], 'end': s['end'], 'media_start': s['media_start'], 'rate': s.get('rate', 1.0)}])})
    if s.get('type') == 'rows':
        y0, gap, h = 500, 178, card_height(1)
        rows_html.append(f'    <div class="stamp" id="stamp-{s["id"]}" style="left:{CARD_X + 6}px;top:{y0 - 112}px">{html.escape(s["stamp"])}</div>')
        for i, r in enumerate(s['rows']):
            segments = r.get('segments') or [{'start': r['enter'], 'end': s['end'], 'media_start': r['media_start'],
                                              'rate': r.get('rate', 1.0)}]
            vids = ''.join(f'''
          <video id="v-lang-{r['lang']}-{k}" class="clip footage" src="{r['footage']}" muted playsinline
            style="left:0;top:{PAD_Y}px;width:{round(1692 * S)}px;height:{round(286 * S)}px"
            data-start="{g['start']}" data-duration="{round(g['end'] - g['start'], 3)}" data-media-start="{g['media_start']}"
            data-track-index="{10 + i}"{f' data-playback-rate="{g["rate"]}"' if g.get('rate') else ''}></video>''' for k, g in enumerate(segments))
            rows_html.append(f'''    <div class="lang-row" id="row-{r['lang']}" style="top:{y0 + i * gap}px">
      <div class="lang-chip" id="chip-{r['lang']}">{html.escape(r['label'])}</div>
      <div class="lang-card" style="height:{h}px">
        <div class="card-mask"><div class="card-cam" id="cam-lang-{r['lang']}" data-layout-allow-overflow>{vids}
        </div></div>
      </div>
    </div>''')
            pans.append({'cam': f"#cam-lang-{r['lang']}", 'keys': pan_keys(Path(r['footage']).name, segments)})

CAPTIONS = []
for i, (vid, a, b, text) in enumerate(E['captions']):
    CAPTIONS.append({'id': f'cap-{i + 1:02d}', 'start': round(E['voice'][vid] + a, 3), 'dur': round(b - a, 3), 'text': text})
caps = '\n'.join(f'    <div id="{c["id"]}" class="clip cap" data-start="{c["start"]}" data-duration="{c["dur"]}" '
                 f'data-track-index="20"><span class="cap-text">{html.escape(c["text"])}</span></div>' for c in CAPTIONS)
config = {'scenes': E['scenes'], 'overlays': E['overlays'], 'total': TOTAL, 'fade': E.get('fade', 0.3),
          'endcard': E['endcard'], 'pans': pans, 'scale': S}

page = f'''<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=1080, height=1920" />
    <title>LokalBot autocomplete — vertical</title>
    <script src="https://cdn.jsdelivr.net/npm/gsap@3.14.2/dist/gsap.min.js"></script>
    <style>
      @font-face {{ font-family: "Newsreader"; src: url("assets/fonts/Newsreader-VF.ttf"); font-weight: 200 800; }}
      @font-face {{ font-family: "Newsreader Italic"; src: url("assets/fonts/Newsreader-Italic-VF.ttf"); font-weight: 200 800; font-style: italic; }}
      @font-face {{ font-family: "Hanken Grotesk"; src: url("assets/fonts/HankenGrotesk-VF.ttf"); font-weight: 100 900; }}
      @font-face {{ font-family: "DM Mono"; src: url("assets/fonts/DMMono-Regular.ttf"); }}
      :root {{ --paper: #F3EDE3; --paper-shade: #E8DFD1; --ink: #24211D; --ink-soft: #5B544B; --accent: #16786C; }}
      * {{ margin: 0; padding: 0; box-sizing: border-box; }}
      html, body {{ width: 1080px; height: 1920px; overflow: hidden; background: var(--paper); }}
      #root {{ position: relative; width: 100%; height: 100%; overflow: hidden; background: var(--paper);
        font-family: "Newsreader", Georgia, serif; color: var(--ink); }}
      #grain {{ position: absolute; inset: -60px; opacity: 0.16; pointer-events: none; }}
      #vignette {{ position: absolute; inset: 0; pointer-events: none;
        background: radial-gradient(ellipse at 50% 42%, rgba(243,237,227,0) 50%, rgba(176,146,108,0.30) 100%); }}
      .card-wrap, .lang-card {{ position: absolute; border-radius: 16px; overflow: hidden; background: #FDFDFD;
        box-shadow: 0 40px 80px -34px rgba(74,48,20,0.55), 0 10px 24px -14px rgba(74,48,20,0.35); }}
      .card-wrap {{ opacity: 0; }}
      /* The text viewport sits inside the card's padding. Its left edge is a
         clean cut (stops land on word starts); the right edge fades so the
         ghost reads as continuing. */
      .card-mask {{ position: absolute; top: 0; bottom: 0; left: {PAD_X}px; right: {PAD_X}px;
        -webkit-mask-image: linear-gradient(90deg, transparent 0, #000 4px, #000 calc(100% - 36px), transparent 100%);
        mask-image: linear-gradient(90deg, transparent 0, #000 4px, #000 calc(100% - 36px), transparent 100%); }}
      .card-cam {{ position: absolute; inset: 0; }}
      .footage {{ position: absolute; }}
      .stamp {{ position: absolute; font-family: "Newsreader Italic", Georgia, serif; font-style: italic;
        font-size: 34px; color: var(--ink-soft); opacity: 0; white-space: nowrap; }}
      .lang-row {{ position: absolute; left: {CARD_X}px; width: {CARD_W}px; height: 170px; opacity: 0; }}
      .lang-chip {{ position: absolute; left: 0; top: 0; height: 44px; padding: 0 20px; border-radius: 999px;
        display: flex; align-items: center; font-family: "Hanken Grotesk", sans-serif; font-size: 24px;
        font-weight: 650; letter-spacing: 0.03em; color: var(--ink); background: var(--paper-shade); }}
      .lang-card {{ left: 0; top: 56px; width: {CARD_W}px; }}
      .cap {{ position: absolute; left: 70px; width: 860px; bottom: 440px; }}
      .cap-text {{ display: block; font-size: 60px; line-height: 1.16; font-weight: 450; color: var(--ink); text-wrap: balance; }}
      #counter {{ position: absolute; left: 70px; top: 800px; opacity: 0; }}
      #counter .num {{ font-size: 230px; font-weight: 520; letter-spacing: -0.02em; line-height: 0.9; }}
      #counter .rule {{ height: 8px; width: 100%; background: var(--accent); margin: 22px 0 22px; transform-origin: left center; }}
      #counter .label {{ font-family: "Hanken Grotesk", sans-serif; font-size: 30px; font-weight: 650; letter-spacing: 0.04em; color: var(--ink-soft); }}
      #counter .source {{ font-family: "Newsreader Italic", Georgia, serif; font-style: italic; font-size: 30px; color: var(--ink-soft); margin-top: 8px; }}
      #memory-tag {{ position: absolute; left: 580px; top: 502px; opacity: 0; font-family: "Hanken Grotesk", sans-serif;
        font-size: 28px; font-weight: 650; color: #F3EDE3; background: var(--accent); padding: 10px 22px; border-radius: 999px; white-space: nowrap; }}
      #memory-note {{ position: absolute; left: 90px; top: 890px; width: 840px; opacity: 0; padding: 34px 38px; background: #FBF8F2;
        border: 2px solid var(--paper-shade); border-radius: 12px; box-shadow: 0 30px 60px -34px rgba(74,48,20,0.5); }}
      #memory-note .kicker {{ font-family: "Hanken Grotesk", sans-serif; font-size: 24px; font-weight: 650; letter-spacing: 0.08em; text-transform: uppercase; color: var(--accent); }}
      #memory-note .title {{ font-size: 40px; font-weight: 560; margin: 12px 0 14px; }}
      #memory-note .quote {{ font-family: "Newsreader Italic", Georgia, serif; font-style: italic; font-size: 34px; line-height: 1.32; }}
      #model-badge {{ position: absolute; left: 60px; top: 1236px; opacity: 0; font-family: "Hanken Grotesk", sans-serif; font-size: 30px;
        font-weight: 500; color: var(--ink); background: #FBF8F2; border: 2px solid var(--accent); padding: 12px 24px; border-radius: 999px; white-space: nowrap; }}
      #model-badge b {{ font-weight: 700; color: var(--accent); }}
      #endcard {{ position: absolute; left: 70px; right: 70px; top: 380px; opacity: 0; }}
      #endcard img {{ width: 190px; height: 190px; }}
      #endcard .word {{ font-size: 156px; font-weight: 560; letter-spacing: -0.02em; line-height: 1; margin-top: 10px; }}
      #endcard .line {{ font-family: "Hanken Grotesk", sans-serif; font-size: 40px; font-weight: 560; line-height: 1.3; margin-top: 34px; text-wrap: balance; }}
      #endcard .model {{ font-family: "DM Mono", monospace; font-size: 27px; color: var(--ink-soft); margin-top: 22px; }}
      #endcard .thanks {{ font-family: "Newsreader Italic", Georgia, serif; font-style: italic; font-size: 34px; line-height: 1.3; color: var(--ink-soft); margin-top: 20px; text-wrap: balance; }}
      #endcard .url {{ font-family: "DM Mono", monospace; font-size: 42px; color: var(--accent); margin-top: 44px; }}
    </style>
  </head>
  <body>
    <div id="root" data-composition-id="main" data-start="0" data-width="1080" data-height="1920" data-duration="{TOTAL}">
      <svg id="grain" width="1200" height="2040" aria-hidden="true">
        <filter id="grain-filter"><feTurbulence type="fractalNoise" baseFrequency="0.85" numOctaves="2" seed="7" stitchTiles="stitch"/>
          <feColorMatrix type="matrix" values="0 0 0 0 0.36  0 0 0 0 0.29  0 0 0 0 0.2  0 0 0 0.55 0"/></filter>
        <rect width="100%" height="100%" filter="url(#grain-filter)"/>
      </svg>
      <div id="vignette"></div>
{''.join(cards)}
{chr(10).join(rows_html)}
      <div id="counter">
        <div class="num">8,176</div>
        <div class="rule"></div>
        <div class="label">TABS ACCEPTED IN COTYPIST SINCE MAY 28</div>
        <div class="source">counted by its own stats file</div>
      </div>
      <div id="model-badge"><b>Gemma 4 E2B</b> base · Q6_K · on this Mac</div>
      <div id="memory-tag">from yesterday's call</div>
      <div id="memory-note">
        <div class="kicker">Meeting note · yesterday, 3:00 pm</div>
        <div class="title">Harbor beta check-in</div>
        <div class="quote">“John confirmed the Harbor beta ships Thursday morning, right after Wednesday’s accessibility review.”</div>
      </div>
      <div id="endcard">
        <img src="assets/brand/lokalbot-mark-brand.svg" alt="" />
        <div class="word">LokalBot</div>
        <div class="line">Free and open source. Every model runs on your Mac.</div>
        <div class="model">autocomplete: Gemma 4 E2B base · Q6_K</div>
        <div class="thanks">Autocomplete built on Cotabby’s open loop, with Cotypist as the bar for how it should feel.</div>
        <div class="url" id="end-url">github.com/stevyhacker/lokalbot</div>
      </div>
{caps}
      <audio id="mix" src="assets/sound/final-mix.wav" data-start="0" data-duration="{mix_dur}" data-track-index="30"></audio>
    </div>
    <script>
      const C = {json.dumps(config, separators=(',', ':'))};
      const tl = gsap.timeline({{ paused: true }});
      tl.fromTo("#grain", {{ x: 0, y: 0 }}, {{ x: -30, y: -40, duration: C.total, ease: "none" }}, 0);
      for (const s of C.scenes) {{
        if (!s.footage) continue;
        const card = "#card-" + s.id, stamp = "#stamp-" + s.id;
        if (s.start === 0) tl.set(card, {{ opacity: 1, y: 0 }}, 0);
        else tl.fromTo(card, {{ opacity: 0, y: 24 }}, {{ opacity: 1, y: 0, duration: C.fade, ease: "power2.out" }}, s.start);
        tl.to(card, {{ opacity: 0, duration: C.fade, ease: "power1.in" }}, s.end - C.fade);
        if (s.start === 0) tl.set(stamp, {{ opacity: 1, x: 0 }}, 0);
        else tl.fromTo(stamp, {{ opacity: 0, x: -10 }}, {{ opacity: 1, x: 0, duration: 0.5, ease: "sine.out" }}, s.start + 0.2);
        tl.to(stamp, {{ opacity: 0, duration: 0.3 }}, s.end - C.fade);
      }}
      for (const p of C.pans) {{
        p.keys.forEach(([t, x], i) => {{
          if (i === 0) tl.set(p.cam, {{ x: -x * C.scale }}, t);
          else tl.to(p.cam, {{ x: -x * C.scale, duration: 0.45, ease: "power2.inOut" }}, Math.max(0, t - 0.15));
        }});
      }}
      for (const s of C.scenes) {{
        if (s.type !== "rows") continue;
        tl.fromTo("#stamp-" + s.id, {{ opacity: 0, x: -10 }}, {{ opacity: 1, x: 0, duration: 0.5, ease: "sine.out" }}, s.start + 0.1);
        tl.to("#stamp-" + s.id, {{ opacity: 0, duration: 0.3 }}, s.end - 0.4);
        s.rows.forEach((r, i) => {{
          const row = "#row-" + r.lang, chip = "#chip-" + r.lang;
          tl.fromTo(row, {{ opacity: 0, x: 90 }}, {{ opacity: 1, x: 0, duration: 0.6, ease: "power3.out" }}, r.enter);
          tl.fromTo(chip, {{ backgroundColor: "#E8DFD1", color: "#24211D" }}, {{ backgroundColor: "#16786C", color: "#F3EDE3", duration: 0.3, ease: "power2.out" }}, r.named);
          tl.fromTo(chip, {{ scale: 1 }}, {{ scale: 1.1, duration: 0.16, ease: "sine.inOut", yoyo: true, repeat: 1 }}, r.named);
          tl.to(row, {{ opacity: 0, y: -14, duration: 0.4, ease: "power1.in" }}, s.end - 0.5 + i * 0.04);
        }});
      }}
      const o = C.overlays;
      if (o.counter.start <= 0.05) {{ tl.set("#counter", {{ opacity: 1, y: 0 }}, 0); tl.set("#counter .rule", {{ scaleX: 1 }}, 0); }}
      else {{
      tl.fromTo("#counter", {{ opacity: 0, y: 30 }}, {{ opacity: 1, y: 0, duration: 0.6, ease: "power3.out" }}, o.counter.start);
      tl.fromTo("#counter .rule", {{ scaleX: 0 }}, {{ scaleX: 1, duration: 0.8, ease: "expo.out" }}, o.counter.start + 0.3);
      }}
      tl.to("#counter", {{ opacity: 0, y: -12, duration: 0.3, ease: "power1.in" }}, o.counter.end - 0.3);
      tl.fromTo("#memory-tag", {{ opacity: 0, y: 12, scale: 0.96 }}, {{ opacity: 1, y: 0, scale: 1, duration: 0.45, ease: "back.out(1.6)" }}, o.memory_tag.start);
      tl.to("#memory-tag", {{ opacity: 0, duration: 0.3 }}, o.memory_tag.end - 0.3);
      tl.fromTo("#memory-note", {{ opacity: 0, y: 40, rotation: 1 }}, {{ opacity: 1, y: 0, rotation: -0.5, duration: 0.6, ease: "power3.out" }}, o.memory_note.start);
      tl.to("#memory-note", {{ opacity: 0, y: 20, duration: 0.3, ease: "power1.in" }}, o.memory_note.end - 0.3);
      tl.fromTo("#model-badge", {{ opacity: 0, y: 12, scale: 0.96 }}, {{ opacity: 1, y: 0, scale: 1, duration: 0.5, ease: "back.out(1.5)" }}, o.model_badge.start);
      tl.to("#model-badge", {{ opacity: 0, duration: 0.3 }}, o.model_badge.end);
      const credit = C.scenes.find(s => s.id === "credit");
      tl.fromTo("#endcard", {{ opacity: 0 }}, {{ opacity: 1, duration: 0.6, ease: "sine.out" }}, credit.start + 0.1);
      tl.fromTo("#endcard img", {{ y: 30, scale: 0.92 }}, {{ y: 0, scale: 1, duration: 0.8, ease: "power3.out" }}, credit.start + 0.1);
      tl.fromTo("#endcard .word", {{ y: 24 }}, {{ y: 0, duration: 0.8, ease: "power3.out" }}, credit.start + 0.2);
      tl.fromTo("#endcard .line", {{ opacity: 0, y: 16 }}, {{ opacity: 1, y: 0, duration: 0.5, ease: "power2.out" }}, credit.start + C.endcard.line);
      tl.fromTo("#endcard .model", {{ opacity: 0, y: 12 }}, {{ opacity: 1, y: 0, duration: 0.5, ease: "power2.out" }}, credit.start + C.endcard.line + 0.8);
      tl.fromTo("#endcard .thanks", {{ opacity: 0, y: 16 }}, {{ opacity: 1, y: 0, duration: 0.5, ease: "power2.out" }}, credit.start + C.endcard.thanks);
      tl.fromTo("#end-url", {{ opacity: 0, y: 14 }}, {{ opacity: 1, y: 0, duration: 0.6, ease: "power3.out" }}, credit.start + C.endcard.url);
      document.querySelectorAll(".cap").forEach(el => {{
        const start = parseFloat(el.dataset.start), dur = parseFloat(el.dataset.duration);
        const text = el.querySelector(".cap-text");
        tl.fromTo(text, {{ opacity: 0, y: 10 }}, {{ opacity: 1, y: 0, duration: 0.2, ease: "power1.out" }}, start);
        tl.to(text, {{ opacity: 0, duration: 0.15 }}, start + dur - 0.15);
      }});
      window.__timelines["main"] = tl;
    </script>
  </body>
</html>
'''
OUT.mkdir(exist_ok=True)
(OUT / 'index.html').write_text(page)
print(f'Built vertical/index.html: {TOTAL:.2f}s, {len(cards)} cards, {len(pans)} panned clips')
for p in pans:
    print(' ', p['cam'], p['keys'])
