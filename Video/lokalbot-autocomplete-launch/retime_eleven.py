"""Time captions and on-screen cues from the chosen ElevenLabs takes.

Reads production/voice-plan.json (chosen files) and the character alignment in
production/eleven-generation.json, then rewrites edit.json's caption offsets,
the language chips' "named" times and the end-card cue times. Voice start
times and scene windows stay as authored in edit.json.
"""
import json
import re
from pathlib import Path

P = Path(__file__).resolve().parent
E = json.loads((P / 'edit.json').read_text())
plan = {t['id']: t['file'] for t in json.loads((P / 'production/voice-plan.json').read_text())['takes']}
rows = {r['file']: r for r in json.loads((P / 'production/eleven-generation.json').read_text())['rows']}


def norm(word):
    word = word.lower().replace('co-typist', 'cotypist').replace('co-tabby', 'cotabby')
    return re.sub(r"[^a-z0-9']", '', word.replace('’', "'"))


def words(take_id):
    a = rows[plan[take_id]]['alignment']
    out, cur, start = [], '', None
    for ch, s, e in zip(a['characters'], a['character_start_times_seconds'], a['character_end_times_seconds']):
        if ch.isspace():
            if cur:
                out.append((norm(cur), start, end))
            cur, start = '', None
            continue
        if start is None:
            start = s
        cur, end = cur + ch, e
    if cur:
        out.append((norm(cur), start, end))
    return [w for w in out if w[0]]


def find(ws, token, after=0.0):
    token = norm(token)
    for w, s, e in ws:
        if s >= after and w == token:
            return s, e
    raise SystemExit(f'"{token}" not found after {after}s')


timings = {}
by_take = {}
for i, (vid, _, _, text) in enumerate(E['captions']):
    by_take.setdefault(vid, []).append(i)
for vid, idxs in by_take.items():
    ws = words(vid)
    timings[vid] = [{'w': w, 's': round(s, 3), 'e': round(e, 3)} for w, s, e in ws]
    starts = []
    cursor = 0.0
    for i in idxs:
        first = E['captions'][i][3].split()[0]
        s, _ = find(ws, first, cursor)
        starts.append(s)
        cursor = s + 0.01
    # The last character's end time runs into the trailing silence, so the
    # final caption also stops before the next line begins.
    later = sorted(t for t in E['voice'].values() if t > E['voice'][vid])
    limit = (later[0] - E['voice'][vid] - 0.12) if later else E['duration'] - E['voice'][vid]
    speech_end = min(ws[-1][1] + 0.6, ws[-1][2])
    for k, i in enumerate(idxs):
        end = starts[k + 1] - 0.08 if k + 1 < len(idxs) else min(speech_end + 0.15, limit)
        E['captions'][i][1] = round(starts[k], 3)
        E['captions'][i][2] = round(end, 3)

voice = E['voice']
lang = next(s for s in E['scenes'] if s['id'] == 'languages')
ws5 = words('05-open')
for r, name in zip(lang['rows'], ['German', 'French', 'Italian', 'Spanish']):
    r['named'] = round(voice['05-open'] + find(ws5, name)[0], 3)

E['overlays']['model_badge']['start'] = round(voice['05-open'] + find(ws5, 'Gemma')[0] - 0.1, 3)

ws7 = words('07-credit')
credit = next(s for s in E['scenes'] if s['id'] == 'credit')
E['endcard'] = {
    'line': round(voice['07-credit'] + find(ws7, 'Free')[0] - credit['start'], 3),
    'thanks': round(voice['07-credit'] + find(ws7, 'It', 4.0)[0] - credit['start'], 3),
    'url': round(voice['08-close'] - 0.3 - credit['start'], 3),
}
(P / 'edit.json').write_text(json.dumps(E, indent=2, ensure_ascii=False) + '\n')
(P / 'production/word-timings.json').write_text(json.dumps(timings, indent=1) + '\n')
print('chips', [r['named'] for r in lang['rows']], 'endcard', E['endcard'])
for c in E['captions']:
    print(f'{c[0]:12} {voice[c[0]] + c[1]:6.2f}-{voice[c[0]] + c[2]:6.2f}  {c[3]}')
