mod mic;
mod state;
mod stt;

use std::path::PathBuf;
use std::time::{Duration, Instant};

// `winarchy-assistant listen <python> <stt/server.py> [segundos]`: graba y transcribe una vez (banco de pruebas de 1.2).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(String::as_str) == Some("levels") {
        return mic::levels(a.get(2).and_then(|s| s.parse().ok()).unwrap_or(6));
    }
    if a.get(1).map(String::as_str) != Some("listen") || a.len() < 4 {
        return;
    }
    let secs: u64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(4);
    let t = Instant::now();
    let mut stt = stt::Stt::spawn(&PathBuf::from(&a[2]), &PathBuf::from(&a[3])).expect("stt");
    println!("modelo listo en {:.1}s; hablá ahora ({secs}s)...", t.elapsed().as_secs_f32());
    let rec = mic::start().expect("mic");
    std::thread::sleep(Duration::from_secs(secs));
    let pcm = rec.stop();
    let rms = (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len().max(1) as f32).sqrt();
    println!("{} muestras ({:.1}s), rms {rms:.4}", pcm.len(), pcm.len() as f32 / 16000.0);
    let per_sec: Vec<String> = pcm.chunks(16000).map(|c| format!("{:.3}", (c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).sqrt())).collect();
    println!("rms por segundo: {}", per_sec.join(" "));
    if mic::is_silent(&pcm) {
        println!("silencio: no se transcribe (Whisper alucina sobre el vacío)");
        return;
    }
    let wav = std::env::temp_dir().join("winarchy-listen.wav");
    mic::write_wav(&wav, &pcm).unwrap();
    let t = Instant::now();
    println!("{:?} en {:.2}s", stt.transcribe(&wav), t.elapsed().as_secs_f32());
    std::fs::remove_file(wav).ok();
}
