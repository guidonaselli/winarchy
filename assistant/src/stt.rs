use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub struct Stt {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
}

impl Stt {
    /// Lanza el sidecar y espera a que el modelo esté caliente (la primera inferencia en CUDA tarda ~20 s).
    pub fn spawn(python: &Path, script: &Path) -> Result<Self, String> {
        let mut child = Command::new(python)
            .arg("-I")
            .arg(script)
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
            return Err(format!("STT did not start: {line}"));
        }
        Ok(Self { child, stdin, out })
    }

    pub fn transcribe(&mut self, wav: &Path) -> Result<String, String> {
        writeln!(self.stdin, "{}", wav.display()).map_err(|e| e.to_string())?;
        let mut line = String::new();
        self.out.read_line(&mut line).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        match v["text"].as_str() {
            Some(t) => Ok(t.to_string()),
            None => Err(v["error"].as_str().unwrap_or("empty reply").to_string()),
        }
    }
}

impl Drop for Stt {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}
