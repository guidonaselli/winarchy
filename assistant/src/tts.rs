use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Media::Audio::{PlaySoundW, SND_FILENAME, SND_NODEFAULT, SND_PURGE, SND_SYNC};

pub struct Tts {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
}

impl Tts {
    pub fn spawn(python: &Path, script: &Path, voice: &Path) -> Result<Self, String> {
        let mut child = Command::new(python)
            .arg("-I")
            .arg(script)
            .env("WINARCHY_VOICE", voice)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let mut out = BufReader::new(child.stdout.take().ok_or("no stdout")?);
        let mut line = String::new();
        out.read_line(&mut line).map_err(|e| e.to_string())?;
        if !line.contains("ready") {
            return Err(format!("TTS did not start: {line}"));
        }
        Ok(Self { child, stdin, out })
    }

    /// Sintetiza una oración y devuelve el WAV (el sidecar rota 8 archivos).
    pub fn synth(&mut self, text: &str) -> Result<PathBuf, String> {
        writeln!(self.stdin, "{}", text.replace(['\r', '\n'], " ")).map_err(|e| e.to_string())?;
        let mut line = String::new();
        self.out.read_line(&mut line).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        v["wav"].as_str().map(PathBuf::from).ok_or_else(|| v["error"].as_str().unwrap_or("empty reply").to_string())
    }
}

impl Drop for Tts {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// Bloquea hasta que termina el audio (o `stop` lo corta).
pub fn play(wav: &Path) {
    let w: Vec<u16> = wav.to_string_lossy().encode_utf16().chain(Some(0)).collect();
    unsafe {
        let _ = PlaySoundW(PCWSTR(w.as_ptr()), HMODULE::default(), SND_FILENAME | SND_SYNC | SND_NODEFAULT);
    }
}

pub fn stop() {
    unsafe {
        let _ = PlaySoundW(PCWSTR::null(), HMODULE::default(), SND_PURGE);
    }
}
