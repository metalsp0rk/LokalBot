"""Word timestamps for the chosen narration takes (faster-whisper base.en), used to time caption chunks."""
import json, subprocess
from pathlib import Path
import numpy as np
from faster_whisper import WhisperModel

P = Path(__file__).resolve().parent
plan = json.loads((P / 'production/voice-plan.json').read_text())['takes']
model = WhisperModel('base.en', device='cpu', compute_type='int8', cpu_threads=4)
out = {}
for t in plan:
    pcm = subprocess.run(['ffmpeg', '-v', 'error', '-i', str(P / t['file']), '-f', 'f32le', '-ac', '1', '-ar', '16000', '-'], check=True, capture_output=True).stdout
    segs, _ = model.transcribe(np.frombuffer(pcm, dtype=np.float32), language='en', beam_size=5, word_timestamps=True)
    out[t['id']] = [{'w': w.word.strip(), 's': round(w.start, 3), 'e': round(w.end, 3)} for s in segs for w in s.words]
(P / 'production/word-timings.json').write_text(json.dumps(out, indent=1) + '\n')
for k, v in out.items(): print(k, ' '.join(f"{x['w']}@{x['s']}" for x in v))
