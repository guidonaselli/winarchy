mod auth;
mod llm;
mod mic;
mod pipe;
mod serve;
mod state;
mod stt;
mod tools;
mod tts;

use std::path::PathBuf;
use std::time::{Duration, Instant};

const HELP: &str = "winarchy-assistant serve | login | logout | status | levels [s] | listen <python> <stt/server.py> [s]";

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let code = match a.get(1).map(String::as_str) {
        Some("serve") => report(serve::run()),
        Some("login") => report(auth::login().map(|_| println!("login ok"))),
        Some("logout") => {
            auth::logout();
            0
        }
        Some("status") => {
            println!("{}", if auth::load().is_some() { "logged in" } else { "not logged in" });
            0
        }
        Some("levels") => {
            mic::levels(a.get(2).and_then(|s| s.parse().ok()).unwrap_or(6));
            0
        }
        Some("listen") if a.len() >= 4 => {
            listen(&a);
            0
        }
        _ => {
            eprintln!("{HELP}");
            2
        }
    };
    std::process::exit(code);
}

fn report(r: Result<(), String>) -> i32 {
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

// Banco de pruebas de la captura: graba y transcribe una vez.
fn listen(a: &[String]) {
    let secs: u64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(4);
    let t = Instant::now();
    let mut stt = stt::Stt::spawn(&PathBuf::from(&a[2]), &PathBuf::from(&a[3])).expect("stt");
    println!("modelo listo en {:.1}s; hablá ahora ({secs}s)...", t.elapsed().as_secs_f32());
    let rec = mic::start().expect("mic");
    std::thread::sleep(Duration::from_secs(secs));
    let pcm = rec.stop();
    if mic::is_silent(&pcm) {
        println!("silencio: no se transcribe");
        return;
    }
    let wav = std::env::temp_dir().join("winarchy-listen.wav");
    mic::write_wav(&wav, &pcm).unwrap();
    let t = Instant::now();
    println!("{:?} en {:.2}s", stt.transcribe(&wav), t.elapsed().as_secs_f32());
    std::fs::remove_file(wav).ok();
}
