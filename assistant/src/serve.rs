use crate::state::{Effect, Event, Machine, State};
use crate::{auth, llm, mic, pipe, stt, tools, tts};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::System::Threading::CreateMutexW;

enum Ev {
    Pipe(pipe::Msg),
    ReplyReady(u64),
    Done(u64),
    Failed(u64, String),
}

/// Los sidecars retienen VRAM y varios GB: se cargan al pedir un turno y se sueltan tras este tiempo sin uso.
const IDLE_UNLOAD: Duration = Duration::from_secs(600);

struct Lazy<T> {
    value: Option<T>,
    used: Instant,
}

impl<T> Lazy<T> {
    fn shared() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self { value: None, used: Instant::now() }))
    }

    /// Corre `f` sobre el valor, cargándolo si hace falta; si falla se descarta para recargarlo en el próximo turno.
    fn with<R>(&mut self, load: impl FnOnce() -> Result<T, String>, f: impl FnOnce(&mut T) -> Result<R, String>) -> Result<R, String> {
        if self.value.is_none() {
            self.value = Some(load()?);
        }
        let result = f(self.value.as_mut().expect("loaded above"));
        self.used = Instant::now();
        if result.is_err() {
            self.value = None;
        }
        result
    }

    fn unload_if_idle(&mut self, now: Instant) {
        if now.saturating_duration_since(self.used) >= IDLE_UNLOAD {
            self.value = None;
        }
    }
}

type SharedStt = Arc<Mutex<Lazy<stt::Stt>>>;
type SharedTts = Arc<Mutex<Lazy<tts::Tts>>>;

fn with_stt<R>(stt: &SharedStt, cfg: &Config, f: impl FnOnce(&mut stt::Stt) -> Result<R, String>) -> Result<R, String> {
    stt.lock().unwrap().with(|| stt::Stt::spawn(&cfg.python, &cfg.stt_script), f)
}

fn with_tts<R>(tts: &SharedTts, cfg: &Config, f: impl FnOnce(&mut tts::Tts) -> Result<R, String>) -> Result<R, String> {
    tts.lock().unwrap().with(|| tts::Tts::spawn(&cfg.python, &cfg.tts_script, &cfg.voice), f)
}

/// Carga ambos en paralelo mientras todavía se está hablando.
fn warm(stt: &SharedStt, tts: &SharedTts, cfg: &Arc<Config>) {
    let (stt, c) = (stt.clone(), cfg.clone());
    std::thread::spawn(move || {
        if let Err(e) = with_stt(&stt, &c, |_| Ok(())) {
            log(&format!("stt: {e}"));
        }
    });
    let (tts, c) = (tts.clone(), cfg.clone());
    std::thread::spawn(move || {
        if let Err(e) = with_tts(&tts, &c, |_| Ok(())) {
            log(&format!("tts: {e}"));
        }
    });
}

struct SessionPidFile {
    path: PathBuf,
    pid: u32,
}

impl SessionPidFile {
    fn create(root: &std::path::Path, session_id: u32, pid: u32) -> Result<Self, String> {
        let path = root.join("state").join(format!("assistant-session-{session_id}"));
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let temp = path.with_extension(format!("tmp-{pid}"));
        std::fs::write(&temp, pid.to_string()).map_err(|e| e.to_string())?;
        if let Err(e) = std::fs::rename(&temp, &path) {
            let _ = std::fs::remove_file(temp);
            return Err(e.to_string());
        }
        Ok(Self { path, pid })
    }
}

impl Drop for SessionPidFile {
    fn drop(&mut self) {
        if std::fs::read_to_string(&self.path).is_ok_and(|value| value.trim() == self.pid.to_string()) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

struct Config {
    lang: &'static str,
    python: PathBuf,
    stt_script: PathBuf,
    tts_script: PathBuf,
    voice: PathBuf,
    model: String,
    effort: String,
}

/// `es` solo si el idioma pedido (o el regional de Windows) es español; cualquier otro cae en inglés.
fn lang_of(locale: &str) -> &'static str {
    if locale.to_lowercase().starts_with("es") {
        "es"
    } else {
        "en"
    }
}

fn locale() -> String {
    let mut b = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(&mut b) };
    String::from_utf16_lossy(&b[..(n.max(1) as usize - 1)])
}

fn config(root: &std::path::Path) -> Config {
    let var = |k: &str| std::env::var(k).ok();
    let state = root.join("state").join("assistant");
    let lang = lang_of(&var("WINARCHY_LANG").unwrap_or_else(locale));
    let (piper, edge) = if lang == "es" { ("es_AR-daniela-high", "es-UY-ValentinaNeural") } else { ("en_US-lessac-high", "en-US-AvaNeural") };
    // Los sidecars heredan el entorno: así comparten el idioma y la voz sin más plomería.
    std::env::set_var("WINARCHY_LANG", lang);
    if var("WINARCHY_EDGE_VOICE").is_none() {
        std::env::set_var("WINARCHY_EDGE_VOICE", edge);
    }
    Config {
        lang,
        python: var("WINARCHY_PY").map(PathBuf::from).unwrap_or_else(|| state.join("venv").join("Scripts").join("python.exe")),
        stt_script: root.join("assistant").join("stt").join("server.py"),
        tts_script: root.join("assistant").join("stt").join("tts_server.py"),
        voice: var("WINARCHY_VOICE").map(PathBuf::from).unwrap_or_else(|| state.join(format!("{piper}.onnx"))),
        model: var("WINARCHY_MODEL").unwrap_or_else(|| "gpt-6-luna".into()),
        effort: var("WINARCHY_EFFORT").unwrap_or_else(|| "low".into()),
    }
}

fn debug(msg: &str) {
    if std::env::var_os("WINARCHY_DEBUG").is_some() {
        log(&format!("debug: {msg}"));
    }
}

pub(crate) fn log(msg: &str) {
    let p = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("Winarchy").join("assistant.log");
    if std::fs::metadata(&p).is_ok_and(|m| m.len() > 512 * 1024) {
        let _ = std::fs::rename(&p, p.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = writeln!(f, "t={t} {msg}");
    }
}

/// Avisa a la isla de esta sesión; la shell puede no estar corriendo, así que cualquier fallo se ignora.
fn publish(s: State, session_id: u32) {
    let path = format!(r"\\.\pipe\winarchy-shell-{session_id}");
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(path) {
        let _ = writeln!(f, r#"{{"v":1,"id":1,"cmd":"assistant.state","params":{{"state":"{}"}}}}"#, s.name());
    }
}

fn current_session_id() -> Option<u32> {
    #[link(name = "kernel32")]
    extern "system" {
        fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> i32;
    }
    let mut session_id = 0;
    let ok = unsafe { ProcessIdToSessionId(std::process::id(), &mut session_id) };
    (ok != 0).then_some(session_id)
}

fn acquire_instance(name: &str) -> Result<Option<HANDLE>, String> {
    let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }.map_err(|e| e.to_string())?;
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { let _ = CloseHandle(handle); }
        return Ok(None);
    }
    Ok(Some(handle))
}

pub fn run() -> Result<(), String> {
    let session_id = current_session_id().ok_or_else(|| "could not determine the Windows session".to_string())?;
    let instance = format!("Local\\WinarchyAssistant-{session_id}");
    let Some(_instance) = acquire_instance(&instance)? else { return Ok(()); };
    let env = tools::env();
    let _pid_file = SessionPidFile::create(&env.root, session_id, std::process::id())?;
    let cfg = Arc::new(config(&env.root));
    let defs = tools::definitions(&env.themes);
    let stt: SharedStt = Lazy::shared();
    let tts: SharedTts = Lazy::shared();
    let llm = Arc::new(Mutex::new(llm::Llm::new(&cfg.model, &cfg.effort, llm::instructions(cfg.lang))));
    let env = Arc::new(env);
    let generation = Arc::new(AtomicU64::new(0));

    let (tx, rx) = channel::<Ev>();
    let (ptx, prx) = channel();
    pipe::serve(session_id, ptx);
    let fwd = tx.clone();
    std::thread::spawn(move || {
        for m in prx {
            let _ = fwd.send(Ev::Pipe(m));
        }
    });

    // Un hilo sintetiza y otro reproduce, en orden: la frase siguiente se sintetiza mientras suena la actual.
    // Lo que quedó de un turno cancelado se descarta por su generación.
    let (stx, srx) = channel::<(u64, Option<String>)>();
    let (wtx, wrx) = channel::<(u64, Option<std::path::PathBuf>)>();
    {
        let (generation, tts, cfg) = (generation.clone(), tts.clone(), cfg.clone());
        std::thread::spawn(move || {
            for (g, sentence) in srx {
                if g != generation.load(Ordering::SeqCst) {
                    continue;
                }
                match sentence {
                    Some(text) => match with_tts(&tts, &cfg, |t| t.synth(&text)) {
                        Ok(wav) => {
                            let _ = wtx.send((g, Some(wav)));
                        }
                        Err(e) => log(&format!("tts: {e}")),
                    },
                    None => {
                        let _ = wtx.send((g, None));
                    }
                }
            }
        });
    }
    {
        let (tx, generation) = (tx.clone(), generation.clone());
        std::thread::spawn(move || {
            for (g, wav) in wrx {
                if g != generation.load(Ordering::SeqCst) {
                    continue;
                }
                match wav {
                    Some(wav) => tts::play(&wav),
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
    publish(published, session_id);
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
                warm(&stt, &tts, &cfg);
                rec = mic::start().map_err(|e| log(&format!("mic: {e}"))).ok();
                if rec.is_none() {
                    m.on(Event::Failed, now);
                }
            }
            Effect::StopCapture => {
                let pcm = rec.take().map(mic::Recording::stop).unwrap_or_default();
                let g = generation.load(Ordering::SeqCst);
                let (tx, stx, stt, cfg, llm, env, defs) = (tx.clone(), stx.clone(), stt.clone(), cfg.clone(), llm.clone(), env.clone(), defs.clone());
                std::thread::spawn(move || {
                    if let Err(e) = turn(g, &pcm, &tx, &stx, &stt, &cfg, &llm, &env, &defs) {
                        let _ = tx.send(Ev::Failed(g, e));
                    }
                });
            }
            Effect::None => {}
        }
        if m.state() == State::Idle {
            // try_lock: una carga en curso no debe frenar el bucle de eventos
            if let Ok(mut s) = stt.try_lock() {
                s.unload_if_idle(now);
            }
            if let Ok(mut t) = tts.try_lock() {
                t.unload_if_idle(now);
            }
        }
        if m.state() != published {
            published = m.state();
            publish(published, session_id);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn turn(g: u64, pcm: &[f32], tx: &Sender<Ev>, stx: &Sender<(u64, Option<String>)>, stt: &SharedStt, cfg: &Config, llm: &Mutex<llm::Llm>, env: &tools::Env, defs: &[serde_json::Value]) -> Result<(), String> {
    let t0 = Instant::now();
    debug(&format!("turn {g}: {} samples", pcm.len()));
    if mic::is_silent(pcm) {
        debug("silent");
        let _ = tx.send(Ev::Done(g));
        return Ok(());
    }
    let wav = std::env::temp_dir().join(format!("winarchy-turn-{g}.wav"));
    mic::write_wav(&wav, pcm).map_err(|e| e.to_string())?;
    let text = with_stt(stt, cfg, |s| s.transcribe(&wav));
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
    let mut llm = llm::Llm::new(&cfg.model, &cfg.effort, llm::instructions(cfg.lang));
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

#[cfg(test)]
mod tests {
    use super::{acquire_instance, lang_of, Lazy, SessionPidFile, IDLE_UNLOAD};
    use std::time::{Duration, Instant};

    #[test]
    fn sidecar_loads_once_is_dropped_after_idle_or_failure() {
        let mut loads = 0;
        let mut lazy: Lazy<u8> = Lazy { value: None, used: Instant::now() };
        for _ in 0..2 {
            lazy.with(|| { loads += 1; Ok(1) }, |_| Ok(())).unwrap();
        }
        assert_eq!(loads, 1);
        lazy.unload_if_idle(lazy.used + IDLE_UNLOAD - Duration::from_secs(1));
        assert_eq!(lazy.value, Some(1));
        lazy.unload_if_idle(lazy.used + IDLE_UNLOAD);
        assert_eq!(lazy.value, None);
        assert!(lazy.with(|| Ok(2), |_| Err::<(), _>("dead".into())).is_err());
        assert_eq!(lazy.value, None);
        assert!(lazy.with(|| Err("no python".into()), |_| Ok(())).is_err());
        assert_eq!(lazy.value, None);
    }

    #[test]
    fn instance_mutex_rejects_a_second_process_before_pid_registration() {
        let name = format!("Local\\WinarchyAssistantTest-{}", std::process::id());
        let first = acquire_instance(&name).expect("first mutex").expect("first owner");
        assert!(acquire_instance(&name).expect("second mutex").is_none());
        unsafe { let _ = super::CloseHandle(first); }
    }

    #[test]
    fn pid_file_is_removed_on_scope_exit() {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("winarchy-assistant-pid-test-{}-{unique}", std::process::id()));
        let pid = std::process::id();
        let file = SessionPidFile::create(&root, 42, pid).expect("PID file");
        let path = root.join("state/assistant-session-42");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), pid.to_string());
        drop(file);
        assert!(!path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pid_file_cleanup_preserves_a_replacement_process_registration() {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("winarchy-assistant-pid-replaced-{}-{unique}", std::process::id()));
        let file = SessionPidFile::create(&root, 42, 4242).expect("PID file");
        let path = root.join("state/assistant-session-42");
        std::fs::write(&path, "4343").unwrap();
        drop(file);
        let content = std::fs::read_to_string(&path);
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(content.unwrap(), "4343");
    }

    #[test]
    fn only_spanish_locales_are_spanish() {
        assert_eq!(lang_of("es-AR"), "es");
        assert_eq!(lang_of("ES"), "es");
        assert_eq!(lang_of("en-US"), "en");
        assert_eq!(lang_of("pt-BR"), "en");
    }
}
