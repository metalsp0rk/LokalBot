"""Generate the narration with local Breeze TTS 2 (audio.cpp, Metal).

Python standard library only. Uses the approved male Breeze voice reference
from the September film and a warmer, slower direction. Each scene gets a few
seeds so the best take can be chosen by ear and by the ASR check.

Usage: python3 generate_breeze.py --runtime /private/tmp/lokalbot-breeze-runtime [--seeds 91,7,23] [--only 03-remember]
"""
import argparse
import array
import base64
import hashlib
import json
import math
import subprocess
import time
import urllib.request
import wave
from pathlib import Path

P = Path(__file__).resolve().parent
SPEC = json.loads((P / 'narration.json').read_text())
MODEL_SHA = '0de52d61560f9f6b2dfeca79f9100f8fce0c2b17c52ec30622e23e150df1ad88'
RUNTIME_SHA = '0bd28d822f6c1a9688c970eaa325f6568bdcbdc6e9ae02eff524ae6408685905'


def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for data in iter(lambda: f.read(4 * 1024 * 1024), b''):
            h.update(data)
    return h.hexdigest()


def write_wav(path, samples):
    with wave.open(str(path), 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(24000)
        w.writeframes(samples.tobytes())


def trim_outer_silence(samples):
    # Keep 100 ms before and 180 ms after detected speech. No inner edits.
    hop = 240
    active = [i for i in range(0, len(samples), hop)
              if math.sqrt(sum(v * v for v in samples[i:i + hop]) / max(1, len(samples[i:i + hop]))) > 110]
    if not active:
        raise RuntimeError('Generated audio has no detectable speech')
    lo = max(0, active[0] - 2400)
    hi = min(len(samples), active[-1] + hop + 4320)
    return samples[lo:hi]


def generate(base, text, output, reference, seed):
    payload = {
        'model': 'breeze', 'input': text, 'stream': True,
        'stream_format': 'sse', 'response_format': 'pcm',
        'voice_ref': str(reference), 'reference_text': SPEC['reference_text'],
        'options': {
            'instruction': SPEC['voice_direction'], 'guidance_scale': 2.5,
            'seed': seed, 'max_tokens': 1500, 'text_chunk_size': 600,
            'stream_frames_per_event': 16, 'stream_lookahead_margin': 12,
        },
    }
    started = time.monotonic()
    req = urllib.request.Request(base + '/v1/audio/speech', json.dumps(payload).encode(),
                                 {'Content-Type': 'application/json', 'Accept': 'text/event-stream'})
    pcm = bytearray()
    done = False
    with urllib.request.urlopen(req, timeout=240) as response:
        for raw in response:
            line = raw.decode().strip()
            if not line.startswith('data:'):
                continue
            value = line[5:].strip()
            if value == '[DONE]':
                break
            event = json.loads(value)
            if event.get('type') == 'speech.audio.delta':
                pcm.extend(base64.b64decode(event['audio']))
            elif event.get('type') == 'speech.audio.done':
                done = True
            elif event.get('type') == 'error':
                raise RuntimeError(event)
    if not pcm or not done:
        raise RuntimeError('Incomplete Breeze generation')
    samples = trim_outer_silence(array.array('h', pcm))
    write_wav(output, samples)
    row = {'text': text, 'seed': seed, 'file': str(output.relative_to(P)),
           'duration': len(samples) / 24000, 'generation_seconds': time.monotonic() - started,
           'peak': max(abs(v) for v in samples) / 32768, 'sha256': sha(output)}
    print(json.dumps(row), flush=True)
    return row


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--port', type=int, default=18656)
    parser.add_argument('--seeds', default='91,7,23')
    parser.add_argument('--only', default='')
    args = parser.parse_args()
    model = args.runtime / 'breeze-tts-2-q8_0.gguf'
    binary = args.runtime / 'runtime/audiocpp_server'
    assert sha(model) == MODEL_SHA, 'Model checksum mismatch'
    assert sha(binary) == RUNTIME_SHA, 'Runtime checksum mismatch'
    reference = P / 'assets/voice/breeze-reference.wav'
    config = {'host': '127.0.0.1', 'port': args.port, 'backend': 'metal', 'device': 0,
              'threads': 4, 'lazy_load': True, 'log_request_body': False, 'max_loaded_models': 1,
              'models': [{'id': 'breeze', 'family': 'breeze_tts', 'path': str(model),
                          'task': 'tts', 'mode': 'streaming',
                          'session_options': {'breeze_tts.attention': 'auto'}}]}
    config_path = P / 'production/breeze-server.json'
    config_path.write_text(json.dumps(config, indent=2) + '\n')
    base = f'http://127.0.0.1:{args.port}'
    takes_dir = P / 'assets/voice/takes'
    takes_dir.mkdir(parents=True, exist_ok=True)
    report_path = P / 'production/breeze-generation.json'
    report = json.loads(report_path.read_text()) if report_path.exists() else {'rows': []}
    report.update({'engine': 'Breeze TTS 2 Q8_0', 'runtime': 'audio.cpp v0.7.3 Metal',
                   'model_sha256': MODEL_SHA, 'runtime_sha256': RUNTIME_SHA,
                   'reference_sha256': sha(reference), 'voice_direction': SPEC['voice_direction']})
    seeds = [int(s) for s in args.seeds.split(',')]
    only = set(filter(None, args.only.split(',')))
    with (P / 'production/breeze-server.log').open('w') as log:
        proc = subprocess.Popen([str(binary), '--config', str(config_path), '--no-ui', '--log'],
                                cwd=binary.parent, stdout=log, stderr=log)
        try:
            for _ in range(150):
                if proc.poll() is not None:
                    raise RuntimeError('Breeze server exited; see production/breeze-server.log')
                try:
                    with urllib.request.urlopen(base + '/health', timeout=1) as r:
                        if r.status == 200:
                            break
                except OSError:
                    time.sleep(.2)
            else:
                raise RuntimeError('Breeze server failed to become ready')
            for scene in SPEC['scenes']:
                if only and scene['id'] not in only:
                    continue
                for seed in seeds:
                    out = takes_dir / f"{scene['id']}-s{seed}.wav"
                    row = generate(base, scene['text'], out, reference, seed)
                    row['id'] = scene['id']
                    report['rows'] = [r for r in report['rows'] if r['file'] != row['file']] + [row]
                    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + '\n')
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    proc.kill()


if __name__ == '__main__':
    main()
