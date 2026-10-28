"""Sidecar de TTS: una línea de texto por stdin, una línea JSON con la ruta del WAV por stdout."""
import json, os, sys, tempfile, wave
from piper import PiperVoice

voice = PiperVoice.load(os.environ["WINARCHY_VOICE"])
out_dir = tempfile.mkdtemp(prefix="winarchy-tts-")

def synth(text, path):
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(voice.config.sample_rate)
        # el dispositivo tarda en despertar y recorta el inicio: colchón de silencio antes y después
        w.writeframes(bytes(2 * voice.config.sample_rate * 3 // 10))
        for chunk in voice.synthesize(text):
            w.writeframes(chunk.audio_int16_bytes)
        w.writeframes(bytes(2 * voice.config.sample_rate // 5))

synth("Hola.", os.path.join(out_dir, "warm.wav"))
print(json.dumps({"ready": True}), flush=True)
n = 0
for line in sys.stdin:
    text = line.strip()
    if not text:
        continue
    n += 1
    path = os.path.join(out_dir, f"s{n % 8}.wav")
    try:
        synth(text, path)
        print(json.dumps({"wav": path}), flush=True)
    except Exception as e:
        print(json.dumps({"error": str(e)}), flush=True)
