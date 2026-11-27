// Supervisa winarchy-uwphost.exe, el helper que presenta las apps UWP cuando Winarchy es el shell de sesión.
// Opt-in por state/uwphost.enabled; con él activo la ventana de shell la registra el helper y no el escritorio.
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE, LPARAM, WPARAM};
use windows::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE};
use windows::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId, SendMessageTimeoutW, SMTO_ABORTIFHUNG, WM_NULL};

const EXE: &str = "winarchy-uwphost.exe";
const BEAT: Duration = Duration::from_secs(2);
const ANSWER_MS: u32 = 1000;
const MISSES: u32 = 3;
// La primera corrida baja los símbolos sin ventana todavía; solo un arranque que no termina nunca se corta.
const START_LIMIT: Duration = Duration::from_secs(600);
const WINDOW: Duration = Duration::from_secs(300);
const BACKOFF: [u64; 3] = [1, 5, 30];
// salida propia del helper: liberó memoria sin apps abiertas
const RECYCLED: u32 = 8;
// no pudo bajar sus símbolos
const UNAVAILABLE: u32 = 7;
// no es un código del helper: nunca anunció que estaba listo
const NEVER_READY: u32 = u32::MAX - 1;

static PRESENTING: AtomicBool = AtomicBool::new(false);

/// El helper está listo para presentar apps UWP ahora mismo.
pub fn presenting() -> bool {
    PRESENTING.load(Ordering::Relaxed)
}

#[derive(Debug, PartialEq)]
enum Next {
    Retry(Duration),
    // símbolos o layout incompatibles: no cambia hasta que cambie twinui
    Block,
    GiveUp,
}

fn after_exit(code: u32, recent_restarts: usize) -> Next {
    match code {
        3 | 4 => Next::Block,
        // 2 ya hay otro host, 5 la ventana de shell es de otro: reintentar no lo arregla
        // un arranque que no termina dura más que la ventana de reintentos: contarlo no lo frenaría
        2 | 5 | NEVER_READY => Next::GiveUp,
        RECYCLED => Next::Retry(Duration::ZERO),
        _ => BACKOFF.get(recent_restarts).map_or(Next::GiveUp, |secs| Next::Retry(Duration::from_secs(*secs))),
    }
}

// un intento sin red puede durar más que la ventana: sus reintentos no vencen o nunca se rendiría
fn expire(restarts: &mut Vec<Instant>, code: u32, now: Instant) {
    if code != UNAVAILABLE {
        restarts.retain(|at| now.duration_since(*at) < WINDOW);
    }
}

#[derive(Default)]
struct Pulse {
    misses: u32,
}

impl Pulse {
    // `answered` es None mientras el helper no anunció que está listo.
    fn hung(&mut self, answered: Option<bool>, age: Duration) -> bool {
        match answered {
            None => age > START_LIMIT,
            Some(answered) => {
                self.misses = if answered { 0 } else { self.misses + 1 };
                self.misses >= MISSES
            }
        }
    }
}

fn exe(root: &Path) -> Option<PathBuf> {
    ["uwphost\\target\\release", "state\\bin"].iter().map(|dir| root.join(dir).join(EXE)).find(|path| path.is_file())
}

fn twinui_stamp() -> String {
    let dir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    std::fs::metadata(Path::new(&dir).join("System32\\twinui.pcshell.dll"))
        .ok()
        .and_then(|m| Some(format!("{}-{}", m.len(), m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs())))
        .unwrap_or_default()
}

fn blocked_marker(root: &Path) -> PathBuf {
    root.join("state").join("uwphost.blocked")
}

fn wanted(enabled: bool, blocked: Option<&str>, stamp: &str) -> bool {
    enabled && blocked.is_none_or(|blocked| blocked.trim() != stamp)
}

/// El helper a lanzar, si esta sesión tiene que delegarle la ventana de shell.
pub fn helper(root: &Path) -> Option<PathBuf> {
    let blocked = std::fs::read_to_string(blocked_marker(root)).ok();
    if !wanted(root.join("state").join("uwphost.enabled").is_file(), blocked.as_deref(), &twinui_stamp()) {
        return None;
    }
    // un bloqueo de otra versión de twinui ya no describe a este equipo
    let _ = std::fs::remove_file(blocked_marker(root));
    exe(root)
}

fn ready(status: &str, pid: u32) -> bool {
    let mut fields = status.split('|');
    fields.next().and_then(|field| field.parse().ok()) == Some(pid) && fields.next() == Some("ready")
}

fn answers(pid: u32) -> bool {
    // SAFETY: Queries on a handle Windows validates; the message has no payload and is time-bounded.
    unsafe {
        let window = GetShellWindow();
        let mut owner = 0;
        GetWindowThreadProcessId(window, Some(&mut owner));
        owner == pid && SendMessageTimeoutW(window, WM_NULL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, ANSWER_MS, None).0 != 0
    }
}

// El job mata al helper si la shell muere de cualquier forma: un huérfano retendría la ventana de shell.
fn job() -> Option<HANDLE> {
    // SAFETY: A new unnamed job; the limit structure outlives the call that reads it.
    unsafe {
        let job = CreateJobObjectW(None, PCWSTR::null()).ok()?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(job, JobObjectExtendedLimitInformation, std::ptr::addr_of!(limits).cast(), std::mem::size_of_val(&limits) as u32).ok()?;
        Some(job)
    }
}

fn spawn(exe: &Path, root: &Path, job: HANDLE) -> std::io::Result<Child> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = Command::new(exe).arg("--parent").arg(std::process::id().to_string()).arg("--root").arg(root).creation_flags(CREATE_NO_WINDOW).spawn()?;
    // SAFETY: Both handles are alive; the child handle is owned by `child`.
    if let Err(error) = unsafe { AssignProcessToJobObject(job, HANDLE(child.as_raw_handle())) } {
        let _ = child.kill();
        let _ = child.wait();
        return Err(std::io::Error::other(error));
    }
    Ok(child)
}

// Devuelve el código de salida; un helper colgado se termina y cuenta como caída.
fn watch(child: &mut Child, root: &Path) -> u32 {
    let (started, mut pulse, mut up) = (Instant::now(), Pulse::default(), false);
    loop {
        std::thread::sleep(BEAT);
        match child.try_wait() {
            Ok(Some(status)) => return status.code().map_or(u32::MAX, |code| code as u32),
            Ok(None) => {}
            Err(_) => return u32::MAX,
        }
        up = up || std::fs::read_to_string(root.join("state").join("uwphost.status")).is_ok_and(|status| ready(&status, child.id()));
        PRESENTING.store(up, Ordering::Relaxed);
        if pulse.hung(up.then(|| answers(child.id())), started.elapsed()) {
            crate::boot_log(root, "uwphost hung, terminate");
            let _ = child.kill();
            let _ = child.wait();
            return if up { u32::MAX } else { NEVER_READY };
        }
    }
}

/// Mantiene vivo el helper desde un hilo propio; `degraded` avisa una sola vez que ya no se va a relanzar.
pub fn supervise(exe: PathBuf, root: PathBuf, degraded: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        let Some(job) = job() else {
            crate::boot_log(&root, "uwphost: no job object, not launched");
            return degraded();
        };
        let mut restarts: Vec<Instant> = Vec::new();
        loop {
            let code = match spawn(&exe, &root, job) {
                Ok(mut child) => {
                    let code = watch(&mut child, &root);
                    PRESENTING.store(false, Ordering::Relaxed);
                    code
                }
                Err(error) => {
                    crate::boot_log(&root, &format!("uwphost spawn failed: {error}"));
                    break;
                }
            };
            expire(&mut restarts, code, Instant::now());
            let next = after_exit(code, restarts.len());
            crate::boot_log(&root, &format!("uwphost exited code={code} next={next:?}"));
            match next {
                Next::Retry(delay) => {
                    std::thread::sleep(delay);
                    if code != RECYCLED {
                        restarts.push(Instant::now());
                    }
                }
                Next::Block => {
                    if let Err(error) = std::fs::write(blocked_marker(&root), twinui_stamp()) {
                        crate::boot_log(&root, &format!("uwphost: cannot write blocked marker: {error}"));
                    }
                    break;
                }
                Next::GiveUp => break,
            }
        }
        degraded();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crashes_back_off_and_give_up_on_the_fourth() {
        let delays: Vec<Next> = (0..4).map(|restarts| after_exit(6, restarts)).collect();
        assert_eq!(delays, [Next::Retry(Duration::from_secs(1)), Next::Retry(Duration::from_secs(5)), Next::Retry(Duration::from_secs(30)), Next::GiveUp]);
        // 0 con la shell viva, 1 (matado desde el Administrador de tareas) y un NTSTATUS también son caídas
        assert_eq!(after_exit(0, 0), Next::Retry(Duration::from_secs(1)));
        assert_eq!(after_exit(1, 0), Next::Retry(Duration::from_secs(1)));
        assert_eq!(after_exit(0xC000_0005, 1), Next::Retry(Duration::from_secs(5)));
    }

    #[test]
    fn permanent_codes_are_not_retried() {
        assert_eq!([after_exit(3, 0), after_exit(4, 0)], [Next::Block, Next::Block]);
        assert_eq!([after_exit(2, 0), after_exit(5, 0)], [Next::GiveUp, Next::GiveUp]);
        assert_eq!(after_exit(NEVER_READY, 0), Next::GiveUp);
        // reciclado: de inmediato aun con el cupo agotado; sin red: reintenta y no bloquea
        assert_eq!(after_exit(RECYCLED, 3), Next::Retry(Duration::ZERO));
        assert_eq!(after_exit(7, 0), Next::Retry(Duration::from_secs(1)));
    }

    #[test]
    fn old_restarts_expire_except_for_a_start_without_network() {
        let first = Instant::now();
        let mut restarts = vec![first];
        expire(&mut restarts, UNAVAILABLE, first + WINDOW * 2);
        assert_eq!(restarts.len(), 1);
        expire(&mut restarts, 6, first + WINDOW * 2);
        assert!(restarts.is_empty());
    }

    #[test]
    fn three_missed_beats_in_a_row_mean_hung() {
        let mut pulse = Pulse::default();
        let age = Duration::from_secs(20);
        let beats = [false, false, true, false, false, false].map(|answered| pulse.hung(Some(answered), age));
        assert_eq!(beats, [false, false, false, false, false, true]);
    }

    #[test]
    fn startup_has_its_own_deadline() {
        let mut pulse = Pulse::default();
        assert!(!pulse.hung(None, Duration::from_secs(120)));
        assert!(pulse.hung(None, START_LIMIT + Duration::from_secs(1)));
    }

    #[test]
    fn readiness_must_come_from_this_process() {
        assert!(ready("42|ready|10.0|", 42));
        assert!(!ready("41|ready|10.0|", 42));
        assert!(!ready("42|starting|10.0|", 42));
    }

    #[test]
    fn only_the_opt_in_without_a_current_block_delegates() {
        assert!(!wanted(false, None, "a"));
        assert!(wanted(true, None, "a"));
        assert!(!wanted(true, Some("a\r\n"), "a"));
        assert!(wanted(true, Some("old"), "a"));
    }

    #[test]
    fn the_job_object_is_available() {
        assert!(job().is_some());
    }
}
