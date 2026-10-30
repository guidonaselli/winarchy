use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

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

#[derive(Clone)]
pub struct Media {
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[derive(Clone, Copy)]
pub enum MediaCmd {
    Prev,
    Toggle,
    Next,
}

fn media_session() -> Option<windows::Media::Control::GlobalSystemMediaTransportControlsSession> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Mgr;
    Mgr::RequestAsync().ok()?.get().ok()?.GetCurrentSession().ok()
}

// Sesión multimedia actual del sistema, sonando o en pausa.
pub fn media() -> Option<Media> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as St;
    let s = media_session()?;
    let p = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let title = p.Title().ok()?.to_string();
    let playing = s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).is_ok_and(|st| st == St::Playing);
    (!title.is_empty()).then(|| Media { title, artist: p.Artist().map(|a| a.to_string()).unwrap_or_default(), playing })
}

pub fn media_control(cmd: MediaCmd) {
    let Some(s) = media_session() else { return };
    let _ = match cmd {
        MediaCmd::Prev => s.TrySkipPreviousAsync(),
        MediaCmd::Toggle => s.TryTogglePlayPauseAsync(),
        MediaCmd::Next => s.TrySkipNextAsync(),
    }
    .and_then(|op| op.get());
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
