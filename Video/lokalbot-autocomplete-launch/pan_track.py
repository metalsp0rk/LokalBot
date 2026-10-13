"""Measure where the text is in each footage clip, for the vertical cut's camera.

For every clip in <cut>/assets/footage it finds the text line bands (rows that
ever hold ink, on TextEdit's 48 px line pitch) and, ten times a second, the
rightmost typed (dark) column and the rightmost inked column including the
gray ghost on each band. Writes <cut>/production/pan-tracks.json. The vertical
builder follows the typed text (the caret) and keeps some ghost in view.

Usage: CUT_DIR=<cut folder> python3 pan_track.py
"""
import json
import os
import subprocess
from pathlib import Path

import numpy as np

P = Path(os.environ.get('CUT_DIR') or Path(__file__).resolve().parent).resolve()
W, H, FPS = 1692, 286, 10


def frames(path):
    raw = subprocess.run(['ffmpeg', '-v', 'error', '-i', str(path), '-vf', f'fps={FPS},format=gray',
                          '-f', 'rawvideo', '-'], check=True, capture_output=True).stdout
    return np.frombuffer(raw, dtype=np.uint8).reshape(-1, H, W)


def bands(ink_rows):
    # TextEdit at 40 pt sets lines on a 48 px pitch; every crop starts just
    # below the ruler, so baselines sit at 37, 85, 133, ... Use each line's
    # letter body above the baseline: descenders of the line above stay out.
    out = []
    for baseline in range(37, H, 48):
        y0, y1 = baseline - 28, baseline
        if ink_rows[y0:y1 + 1].sum() > 0:
            out.append([y0, y1])
    return out


tracks = {}
for clip in sorted((P / 'assets/footage').glob('*.mp4')):
    f = frames(clip)
    ink = f < 235
    dark = f < 110
    ink[:, :, W - 24:] = False          # ignore the window's right margin
    dark[:, :, W - 24:] = False
    found = bands(ink.any(axis=0).sum(axis=1))
    samples = []
    for i in range(len(f)):
        row = []
        for y0, y1 in found:
            d = np.where(dark[i, y0:y1 + 1].any(axis=0))[0]
            a = np.where(ink[i, y0:y1 + 1].any(axis=0))[0]
            row.append([int(d.max()) if d.size else -1, int(a.max()) if a.size else -1])
        samples.append([round(i / FPS, 2), row])
    tracks[clip.name] = {'bands': found, 'samples': samples}
    print(clip.name, 'bands', found)
(P / 'production/pan-tracks.json').write_text(json.dumps(tracks) + '\n')
