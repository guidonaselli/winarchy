"""Sidecar de TTS: una línea de texto por stdin, una línea JSON con la ruta del WAV por stdout.
Voz neural de Edge si hay red (WINARCHY_TTS=piper la apaga); Piper local como respaldo."""
import asyncio, json, os, sys, tempfile, wave
import edge_tts, miniaudio
from piper import PiperVoice

EDGE_VOICE = os.environ.get("WINARCHY_EDGE_VOICE", "es-AR-ElenaNeural")
EDGE = os.environ.get("WINARCHY_TTS", "edge") != "piper"
EDGE_RATE = 24000

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

def pad(w, rate, seconds):
    w.writeframes(bytes(int(2 * rate * seconds)))

def synth_edge(text, path):
    mp3 = path + ".mp3"
    asyncio.run(edge_tts.Communicate(text, EDGE_VOICE).save(mp3))
    pcm = miniaudio.decode_file(mp3, output_format=miniaudio.SampleFormat.SIGNED16, nchannels=1, sample_rate=EDGE_RATE)
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(EDGE_RATE)
        pad(w, EDGE_RATE, 0.3)
        w.writeframes(pcm.samples.tobytes())
        pad(w, EDGE_RATE, 0.2)

def speak(text, path):
    if EDGE:
        try:
            return synth_edge(text, path)
        except Exception as e:
            print(json.dumps({"fallback": str(e)}), file=sys.stderr, flush=True)
    synth(text, path)

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
        speak(text, path)
        print(json.dumps({"wav": path}), flush=True)
    except Exception as e:
        print(json.dumps({"error": str(e)}), flush=True)
