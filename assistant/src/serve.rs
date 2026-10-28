use crate::state::{Effect, Event, Machine, State};
use crate::{auth, llm, mic, pipe, stt, tools, tts};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

enum Ev {
    Pipe(pipe::Msg),
    ReplyReady(u64),
    Done(u64),
    Failed(u64, String),
}

struct Config {
    python: PathBuf,
    stt_script: PathBuf,
    tts_script: PathBuf,
    voice: PathBuf,
    model: String,
    effort: String,
}

fn config(root: &std::path::Path) -> Config {
    let var = |k: &str| std::env::var(k).ok();
    let state = root.join("state").join("assistant");
    Config {
        python: var("WINARCHY_PY").map(PathBuf::from).unwrap_or_else(|| state.join("venv").join("Scripts").join("python.exe")),
        stt_script: root.join("assistant").join("stt").join("server.py"),
        tts_script: root.join("assistant").join("stt").join("tts_server.py"),
        voice: var("WINARCHY_VOICE").map(PathBuf::from).unwrap_or_else(|| state.join("es_AR-daniela-high.onnx")),
        model: var("WINARCHY_MODEL").unwrap_or_else(|| "gpt-6-luna".into()),
        effort: var("WINARCHY_EFFORT").unwrap_or_else(|| "low".into()),
    }
}

fn debug(msg: &str) {
    if std::env::var_os("WINARCHY_DEBUG").is_some() {
        log(&format!("debug: {msg}"));
    }
}

fn log(msg: &str) {
    let p = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("Winarchy").join("assistant.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{msg}");
    }
}

/// Avisa a la isla; la shell puede no estar corriendo, así que cualquier fallo se ignora.
fn publish(s: State) {
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(r"\\.\pipe\winarchy-shell") {
        let _ = writeln!(f, r#"{{"v":1,"id":1,"cmd":"assistant.state","params":{{"state":"{}"}}}}"#, s.name());
    }
}

pub fn run() -> Result<(), String> {
    let env = tools::env();
    let cfg = config(&env.root);
    let defs = tools::definitions(&env.themes);
    let stt = Arc::new(Mutex::new(stt::Stt::spawn(&cfg.python, &cfg.stt_script)?));
    let mut tts = tts::Tts::spawn(&cfg.python, &cfg.tts_script, &cfg.voice)?;
    let llm = Arc::new(Mutex::new(llm::Llm::new(&cfg.model, &cfg.effort)));
    let env = Arc::new(env);
    let generation = Arc::new(AtomicU64::new(0));

    let (tx, rx) = channel::<Ev>();
    let (ptx, prx) = channel();
    pipe::serve(ptx);
    let fwd = tx.clone();
    std::thread::spawn(move || {
        for m in prx {
            let _ = fwd.send(Ev::Pipe(m));
        }
    });

    // Un solo hilo habla, en orden; lo que quedó de un turno cancelado se descarta por su generación.
    let (stx, srx) = channel::<(u64, Option<String>)>();
    {
        let (tx, generation) = (tx.clone(), generation.clone());
        std::thread::spawn(move || {
            for (g, sentence) in srx {
                if g != generation.load(Ordering::SeqCst) {
                    continue;
                }
                match sentence {
                    Some(text) => match tts.synth(&text) {
                        Ok(wav) if g == generation.load(Ordering::SeqCst) => tts::play(&wav),
                        Ok(_) => {}
                        Err(e) => log(&format!("tts: {e}")),
                    },
                    None => {
                        let _ = tx.send(Ev::Done(g));
                    }
                }
            }
        });
    }

    let mut m = Machine::new(Instant::now());
    let mut rec: Option<mic::Recording> = None;
    let mut published = State::Idle;
    publish(published);
    loop {
        let now = Instant::now();
        let (effect, quit) = match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(Ev::Pipe(pipe::Msg::Down)) => (m.on(Event::PttDown, now), false),
            Ok(Ev::Pipe(pipe::Msg::Up)) => (m.on(Event::PttUp, now), false),
            Ok(Ev::Pipe(pipe::Msg::Quit)) => (Effect::None, true),
            Ok(Ev::ReplyReady(g)) if g == generation.load(Ordering::SeqCst) => (m.on(Event::ReplyReady, now), false),
            Ok(Ev::Done(g)) if g == generation.load(Ordering::SeqCst) => (m.on(Event::SpeechDone, now), false),
            Ok(Ev::Failed(g, e)) if g == generation.load(Ordering::SeqCst) => {
                log(&format!("turn failed: {e}"));
                (m.on(Event::Failed, now), false)
            }
            Ok(_) | Err(RecvTimeoutError::Timeout) => (m.tick(now), false),
            Err(RecvTimeoutError::Disconnected) => (Effect::None, true),
        };
        if quit {
            tts::stop();
            return Ok(());
        }
        match effect {
            Effect::StartCapture | Effect::CancelAndCapture => {
                generation.fetch_add(1, Ordering::SeqCst);
                tts::stop();
                rec = mic::start().map_err(|e| log(&format!("mic: {e}"))).ok();
                if rec.is_none() {
                    m.on(Event::Failed, now);
                }
            }
            Effect::StopCapture => {
                let pcm = rec.take().map(mic::Recording::stop).unwrap_or_default();
                let g = generation.load(Ordering::SeqCst);
                let (tx, stx, stt, llm, env, defs) = (tx.clone(), stx.clone(), stt.clone(), llm.clone(), env.clone(), defs.clone());
                std::thread::spawn(move || {
                    if let Err(e) = turn(g, &pcm, &tx, &stx, &stt, &llm, &env, &defs) {
                        let _ = tx.send(Ev::Failed(g, e));
                    }
                });
            }
            Effect::None => {}
        }
        if m.state() != published {
            published = m.state();
            publish(published);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn turn(g: u64, pcm: &[f32], tx: &Sender<Ev>, stx: &Sender<(u64, Option<String>)>, stt: &Mutex<stt::Stt>, llm: &Mutex<llm::Llm>, env: &tools::Env, defs: &[serde_json::Value]) -> Result<(), String> {
    let t0 = Instant::now();
    debug(&format!("turn {g}: {} samples", pcm.len()));
    if mic::is_silent(pcm) {
        debug("silent");
        let _ = tx.send(Ev::Done(g));
        return Ok(());
    }
    let wav = std::env::temp_dir().join(format!("winarchy-turn-{g}.wav"));
    mic::write_wav(&wav, pcm).map_err(|e| e.to_string())?;
    let text = stt.lock().unwrap().transcribe(&wav);
    let _ = std::fs::remove_file(&wav);
    let text = text?;
    debug(&format!("stt: {} chars at {:.2}s", text.len(), t0.elapsed().as_secs_f32()));
    if text.trim().is_empty() {
        let _ = tx.send(Ev::Done(g));
        return Ok(());
    }
    let token = auth::token()?;
    debug("llm start");
    let mut first = true;
    llm.lock().unwrap().turn(
        &token,
        &text,
        defs,
        &mut |s| {
            if first {
                first = false;
                debug(&format!("first sentence at {:.2}s", t0.elapsed().as_secs_f32()));
                let _ = tx.send(Ev::ReplyReady(g));
            }
            let _ = stx.send((g, Some(s.to_string())));
        },
        &mut |name, args| {
            let r = tools::run(name, args, env);
            log(&format!("tool {name} {args} -> {}", r.text.chars().take(200).collect::<String>()));
            r
        },
    )?;
    debug("llm done");
    let _ = stx.send((g, None));
    Ok(())
}

/// Banco de pruebas: un turno por texto (sin micrófono ni STT) con LLM, acciones y voz reales.
pub fn ask(text: &str) -> Result<(), String> {
    let env = tools::env();
    let cfg = config(&env.root);
    let mut tts = tts::Tts::spawn(&cfg.python, &cfg.tts_script, &cfg.voice)?;
    let mut llm = llm::Llm::new(&cfg.model, &cfg.effort);
    let token = auth::token()?;
    let t0 = Instant::now();
    let (mut first_text, mut first_audio) = (None, None);
    let defs = tools::definitions(&env.themes);
    let out = llm.turn(
        &token,
        text,
        &defs,
        &mut |s| {
            first_text.get_or_insert(t0.elapsed());
            println!("[{:.2}s] {s}", t0.elapsed().as_secs_f32());
            if let Ok(wav) = tts.synth(s) {
                first_audio.get_or_insert(t0.elapsed());
                tts::play(&wav);
            }
        },
        &mut |name, args| {
            let r = tools::run(name, args, &env);
            println!("[tool] {name} {args} -> {}", r.text);
            r
        },
    )?;
    println!("texto en {:?}, primer audio en {:?}, total {:.2}s :: {out}", first_text, first_audio, t0.elapsed().as_secs_f32());
    Ok(())
}
