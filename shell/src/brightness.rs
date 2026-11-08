// Brillo de los monitores por DDC/CI a través del CLI de PowerToys PowerDisplay (dependencia opcional: sin él no hay medidor).
// Cada llamada al CLI tarda ~180 ms, así que corre en un hilo propio, con los pasos de la rueda acumulados, nunca desde la UI.
// shortcut: el medidor muestra el promedio de todos los monitores y fijar un valor los iguala; mejorar con un monitor por medidor si molesta.

use serde_json::Value;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const STALE: Duration = Duration::from_secs(4);
const STEP: i32 = 5;
const CLI: &str = "PowerToys.PowerDisplay.CLI.exe";

struct Shared {
    monitors: Vec<u32>,
    avg: Option<u32>,
    last: Option<Instant>,
    busy: bool,
    pending: i32,
    set_to: Option<u32>,
    refresh: bool,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { monitors: Vec::new(), avg: None, last: None, busy: false, pending: 0, set_to: None, refresh: false });
static NOTIFY: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

// `notify` avisa a la UI que hay un valor nuevo.
pub fn init(notify: impl Fn() + Send + Sync + 'static) {
    let _ = NOTIFY.set(Box::new(notify));
}

fn exe() -> Option<&'static PathBuf> {
    static EXE: OnceLock<Option<PathBuf>> = OnceLock::new();
    EXE.get_or_init(|| {
        let local = std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("PowerToys").join("bin").join(CLI));
        local.filter(|p| p.is_file()).or_else(|| std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(CLI)).find(|p| p.is_file()))
    })
    .as_ref()
}

// (número de monitor, porcentaje) por cada monitor de la respuesta de `get`.
fn parse_levels(json: &str) -> Vec<(u32, u32)> {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    v["monitors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let n = m["monitor"]["number"].as_u64()? as u32;
            let pct = m["settings"][0]["display"].as_str()?.trim_end_matches('%').parse().ok()?;
            Some((n, pct))
        })
        .collect()
}

fn run(args: &[String]) -> Option<String> {
    let out = std::process::Command::new(exe()?).arg("--json").args(args).creation_flags(CREATE_NO_WINDOW).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn fetch() {
    let levels = run(&args(&["get", "--setting", "brightness"])).map(|s| parse_levels(&s)).unwrap_or_default();
    if levels.is_empty() {
        return;
    }
    let mut s = SHARED.lock().unwrap();
    s.monitors = levels.iter().map(|l| l.0).collect();
    s.avg = Some((levels.iter().map(|l| l.1).sum::<u32>() as f32 / levels.len() as f32).round() as u32);
}

fn monitors_arg() -> String {
    let m = SHARED.lock().unwrap().monitors.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    if m.is_empty() { "1".into() } else { m }
}

enum Job {
    Set(u32),
    Adjust(i32),
    Get,
}

fn worker() {
    loop {
        let job = {
            let mut s = SHARED.lock().unwrap();
            if let Some(p) = s.set_to.take() {
                Job::Set(p)
            } else if s.pending != 0 {
                Job::Adjust(std::mem::take(&mut s.pending))
            } else if s.refresh {
                s.refresh = false;
                Job::Get
            } else {
                s.busy = false;
                return;
            }
        };
        if !matches!(job, Job::Get) && SHARED.lock().unwrap().monitors.is_empty() {
            fetch();
        }
        let n = monitors_arg();
        match job {
            Job::Set(p) => drop(run(&args(&["set", "--brightness", &p.to_string(), "-n", &n]))),
            Job::Adjust(d) => drop(run(&args(&[if d > 0 { "up" } else { "down" }, "--brightness", "--step", &d.abs().to_string(), "-n", &n]))),
            Job::Get => {}
        }
        fetch();
        if let Some(f) = NOTIFY.get() {
            f();
        }
    }
}

fn kick(s: &mut Shared) {
    if !s.busy {
        s.busy = true;
        std::thread::spawn(worker);
    }
}

pub fn level() -> Option<u32> {
    exe()?;
    SHARED.lock().unwrap().avg
}

pub fn refresh_if_stale() {
    if exe().is_none() {
        return;
    }
    let mut s = SHARED.lock().unwrap();
    if s.last.is_none_or(|t| t.elapsed() > STALE) {
        s.last = Some(Instant::now());
        s.refresh = true;
        kick(&mut s);
    }
}

// `notches` con signo: cada uno mueve STEP puntos.
pub fn adjust(notches: i32) {
    let mut s = SHARED.lock().unwrap();
    s.pending += notches * STEP;
    s.last = Some(Instant::now());
    kick(&mut s);
}

pub fn set(percent: u32) {
    let mut s = SHARED.lock().unwrap();
    s.set_to = Some(percent.min(100));
    s.last = Some(Instant::now());
    kick(&mut s);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_monitor_from_the_get_answer() {
        let body = r#"{"isError":false,"version":"1.0","command":"get","monitors":[{"monitor":{"number":1,"name":"M2763"},"settings":[{"setting":"brightness","display":"29%","supported":true}]},{"monitor":{"number":2,"name":"MSI G241"},"settings":[{"setting":"brightness","display":"80%","supported":true}]}]}"#;
        assert_eq!(parse_levels(body), [(1, 29), (2, 80)]);
    }

    #[test]
    fn errors_and_garbage_yield_no_levels() {
        assert!(parse_levels(r#"{"isError":true,"error":{"code":"ARGUMENT_ERROR"}}"#).is_empty());
        assert!(parse_levels("no json").is_empty());
        assert!(parse_levels(r#"{"monitors":[{"monitor":{"number":1},"settings":[{"display":"n/a"}]}]}"#).is_empty());
    }
}
