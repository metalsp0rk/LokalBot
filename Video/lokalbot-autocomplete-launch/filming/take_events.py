"""Timeline of ink (typed text) and ghost (gray suggestion) pixels in a take.

Usage: python3 take_events.py <take.mov> [x y w h]  (region in recording pixels)
Prints one row per 0.2 s: time, dark pixel count, gray pixel count.
"""
import subprocess, sys
import numpy as np

path = sys.argv[1]
x, y, w, h = (int(v) for v in sys.argv[2:6]) if len(sys.argv) > 5 else (860, 470, 1720, 300)
fps = 5
cmd = ['ffmpeg', '-v', 'error', '-i', path, '-vf', f'fps={fps},crop={w}:{h}:{x}:{y},format=gray',
       '-f', 'rawvideo', '-']
raw = subprocess.run(cmd, check=True, capture_output=True).stdout
frames = np.frombuffer(raw, dtype=np.uint8).reshape(-1, h, w)
prev = None
for i, f in enumerate(frames):
    dark = int((f < 70).sum())
    gray = int(((f > 120) & (f < 175)).sum())
    print(f'{i / fps:6.1f} dark={dark:6d} gray={gray:6d}')
