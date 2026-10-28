"""Sidecar de STT: una línea con la ruta de un WAV 16 kHz mono por stdin, una línea JSON por stdout."""
import json, os, sys, wave
import numpy as np
try:
    import nvidia.cublas, nvidia.cudnn
    for m in (nvidia.cublas, nvidia.cudnn):
        d = os.path.join(list(m.__path__)[0], "bin")
        os.add_dll_directory(d)
        os.environ["PATH"] = d + os.pathsep + os.environ["PATH"]
except ImportError:
    pass
import ctranslate2
from faster_whisper import WhisperModel

gpu = ctranslate2.get_cuda_device_count() > 0
model = WhisperModel(os.environ.get("WINARCHY_STT_MODEL", "large-v3-turbo" if gpu else "small"), device="cuda" if gpu else "cpu", compute_type="float16" if gpu else "int8")

def read(path):
    with wave.open(path) as w:
        return np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(np.float32) / 32768

LANG = os.environ.get("WINARCHY_LANG", "es")
HINT = {
    "es": "Winarchy: abrí una terminal, pasá al workspace dos, cambiá el tema, komorebi, WezTerm.",
    "en": "Winarchy: open a terminal, go to workspace two, change the theme, komorebi, WezTerm.",
}[LANG]

list(model.transcribe(np.zeros(16000, np.float32), language=LANG, beam_size=5, initial_prompt=HINT)[0])
print(json.dumps({"ready": True}), flush=True)
for line in sys.stdin:
    try:
        segs, _ = model.transcribe(read(line.strip()), language=LANG, beam_size=5, initial_prompt=HINT)
        print(json.dumps({"text": " ".join(s.text for s in segs).strip()}), flush=True)
    except Exception as e:
        print(json.dumps({"error": str(e)}), flush=True)
