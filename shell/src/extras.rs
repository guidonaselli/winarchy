use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub static WEATHER: Mutex<Option<String>> = Mutex::new(None);

pub fn weather_text(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    Some(format!("{:.0}°C", v["current"]["temperature_2m"].as_f64()?))
}

// shortcut: toma la ubicación del caché de YASB, sin su archivo no hay clima; mover a config propia al retirar YASB.
fn coords() -> Option<(f64, f64)> {
    let p = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("YASB").join("weather.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(p).ok()?).ok()?;
    Some((v["weather"]["latitude"].as_f64()?, v["weather"]["longitude"].as_f64()?))
}

// Open-Meteo vía curl.exe (viene con Windows 11): sin dependencia HTTP; una consulta cada 30 min.
pub fn spawn_weather() {
    std::thread::spawn(|| loop {
        if let Some((lat, lon)) = coords() {
            let url = format!("https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m");
            let out = std::process::Command::new("curl.exe").args(["-s", "--max-time", "10", &url]).creation_flags(CREATE_NO_WINDOW).output();
            if let Some(t) = out.ok().and_then(|o| weather_text(&String::from_utf8_lossy(&o.stdout))) {
                *WEATHER.lock().unwrap() = Some(t);
            }
        }
        std::thread::sleep(Duration::from_secs(1800));
    });
}

// "Título - Artista" de la sesión multimedia actual si está reproduciendo.
pub fn media() -> Option<String> {
    use windows::Media::Control::{GlobalSystemMediaTransportControlsSessionManager as Mgr, GlobalSystemMediaTransportControlsSessionPlaybackStatus as St};
    let s = Mgr::RequestAsync().ok()?.get().ok()?.GetCurrentSession().ok()?;
    if s.GetPlaybackInfo().ok()?.PlaybackStatus().ok()? != St::Playing {
        return None;
    }
    let p = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let (t, a) = (p.Title().ok()?.to_string(), p.Artist().ok()?.to_string());
    let line = if a.is_empty() { t } else { format!("{t} - {a}") };
    Some(line.chars().take(48).collect())
}

fn recent_files(dir: &Path, since: SystemTime, depth: u8) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() && depth > 0 {
                recent_files(&p, since, depth - 1)
            } else {
                (p.extension().is_some_and(|x| x == "jsonl") && e.metadata().and_then(|m| m.modified()).is_ok_and(|m| m > since)) as usize
            }
        })
        .sum()
}

// Sesiones de Claude y Codex activas en la última hora, contadas por archivos de sesión locales (sin credenciales ni red).
pub struct AiSessions {
    cache: Option<(Instant, String)>,
}

impl AiSessions {
    pub const fn new() -> Self {
        Self { cache: None }
    }

    pub fn label(&mut self) -> Option<String> {
        if let Some((t, s)) = &self.cache {
            if t.elapsed() < Duration::from_secs(30) {
                return Some(s.clone()).filter(|s| !s.is_empty());
            }
        }
        let home = PathBuf::from(std::env::var_os("USERPROFILE")?);
        let since = SystemTime::now() - Duration::from_secs(3600);
        let (c, x) = (recent_files(&home.join(".claude").join("projects"), since, 3), recent_files(&home.join(".codex").join("sessions"), since, 4));
        let text = [("CLAUDE", c), ("CODEX", x)].iter().filter(|(_, n)| *n > 0).map(|(l, n)| format!("{l} {n}")).collect::<Vec<_>>().join("  ");
        self.cache = Some((Instant::now(), text.clone()));
        Some(text).filter(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_meteo_current_temperature() {
        assert_eq!(weather_text(r#"{"current":{"temperature_2m":14.5}}"#).as_deref(), Some("14°C"));
        assert!(weather_text("{}").is_none());
    }
}
