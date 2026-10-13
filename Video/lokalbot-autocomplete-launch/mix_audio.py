"""Mix the narration takes over the piano bed into assets/sound/final-mix.wav.

Voice sits at about -16 LUFS integrated; the bed is ducked under speech and
rises between lines and over the end card. Deterministic ffmpeg graph.
"""
import json
import subprocess
from pathlib import Path

P = Path(__file__).resolve().parent
E = json.loads((P / 'edit.json').read_text())
plan = {t['id']: t['file'] for t in json.loads((P / 'production/voice-plan.json').read_text())['takes']}
total = E['duration']


def duration(path):
    return float(subprocess.run(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', str(path)],
                                check=True, capture_output=True, text=True).stdout)


spans = [(start, start + duration(P / plan[vid])) for vid, start in E['voice'].items()]
# Bed gain: -24 dB under speech, -15 dB in gaps, eased over 0.35 s.
expr = '0.178'
for a, b in spans:
    up = f'clip((t-{a - 0.35:.3f})/0.35,0,1)*clip(({b + 0.35:.3f}-t)/0.35,0,1)'
    expr = f'({expr})*(1-{up})+0.063*{up}'
args = ['ffmpeg', '-v', 'error', '-y']
for vid in E['voice']:
    args += ['-i', str(P / plan[vid])]
args += ['-i', str(P / 'assets/bgm/bed.wav')]
f = []
for i, (vid, start) in enumerate(E['voice'].items()):
    ms = int(round(start * 1000))
    f.append(f'[{i}:a]aresample=48000,pan=stereo|c0=c0|c1=c0,adelay={ms}|{ms}[v{i}]')
n = len(E['voice'])
f.append(''.join(f'[v{i}]' for i in range(n)) + f'amix=inputs={n}:normalize=0,loudnorm=I=-16:TP=-2:LRA=7[vo]')
f.append(f"[{n}:a]atrim=0:{total},volume='{expr}':eval=frame,afade=t=out:st={total - 3.5}:d=3.5[bed]")
f.append(f'[vo][bed]amix=inputs=2:normalize=0,apad=pad_dur=2,atrim=0:{total},loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000,apad=whole_dur={total}[out]')
out = P / 'assets/sound/final-mix.wav'
args += ['-filter_complex', ';'.join(f), '-map', '[out]', '-ar', '48000', '-ac', '2', '-c:a', 'pcm_s16le', '-t', str(total), str(out)]
subprocess.run(args, check=True)
r = subprocess.run(['ffmpeg', '-v', 'info', '-i', str(out), '-af', 'ebur128=peak=true', '-f', 'null', '-'],
                   capture_output=True, text=True).stderr
print(out, [l.strip() for l in r.splitlines() if l.strip().startswith(('I:', 'Peak:', 'LRA:'))][-3:])
