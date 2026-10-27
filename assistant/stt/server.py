"""Sidecar de STT: una línea con la ruta de un WAV 16 kHz mono por stdin, una línea JSON por stdout."""
import json, os, sys, wave
import numpy as np
import nvidia.cublas, nvidia.cudnn
for m in (nvidia.cublas, nvidia.cudnn):
    d = os.path.join(list(m.__path__)[0], "bin")
    os.add_dll_directory(d)
    os.environ["PATH"] = d + os.pathsep + os.environ["PATH"]
from faster_whisper import WhisperModel

model = WhisperModel(os.environ.get("WINARCHY_STT_MODEL", "large-v3-turbo"), device="cuda", compute_type="float16")

def read(path):
    with wave.open(path) as w:
        return np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(np.float32) / 32768

list(model.transcribe(np.zeros(16000, np.float32), language="es", beam_size=1)[0])
print(json.dumps({"ready": True}), flush=True)
for line in sys.stdin:
    try:
        segs, _ = model.transcribe(read(line.strip()), language="es", beam_size=1)
        print(json.dumps({"text": " ".join(s.text for s in segs).strip()}), flush=True)
    except Exception as e:
        print(json.dumps({"error": str(e)}), flush=True)
