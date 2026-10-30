use crate::i18n::{t_in, Lang, K};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, PartialEq)]
pub struct Weather {
    pub temp: i32,
    pub feels: Option<i32>,
    pub humidity: Option<u32>,
    pub wind: Option<u32>,
    pub code: Option<u32>,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub rain: Option<u32>,
    pub lat: f64,
    pub lon: f64,
}

pub static WEATHER: Mutex<Option<Weather>> = Mutex::new(None);

pub fn parse_weather(body: &str, lat: f64, lon: f64) -> Option<Weather> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let (c, d) = (&v["current"], &v["daily"]);
    let round = |x: &serde_json::Value| x.as_f64().map(|f| f.round() as i32);
    Some(Weather {
        temp: round(&c["temperature_2m"])?,
        feels: round(&c["apparent_temperature"]),
        humidity: round(&c["relative_humidity_2m"]).map(|h| h.max(0) as u32),
        wind: round(&c["wind_speed_10m"]).map(|w| w.max(0) as u32),
        code: c["weather_code"].as_u64().map(|c| c as u32),
        min: round(&d["temperature_2m_min"][0]),
        max: round(&d["temperature_2m_max"][0]),
        rain: round(&d["precipitation_probability_max"][0]).map(|r| r.max(0) as u32),
        lat,
        lon,
    })
}

// Códigos WMO de Open-Meteo, agrupados.
fn condition(code: u32, lang: Lang) -> &'static str {
    let k = match code {
        0 => K::CondClear,
        1 | 2 => K::CondPartlyCloudy,
        3 => K::CondOvercast,
        45 | 48 => K::CondFog,
        51..=57 => K::CondDrizzle,
        61..=67 => K::CondRain,
        71..=77 => K::CondSnow,
        80..=82 => K::CondShowers,
        85 | 86 => K::CondSnowShowers,
        95..=99 => K::CondThunderstorm,
        _ => return "",
    };
    t_in(k, lang)
}

impl Weather {
    pub fn title(&self, lang: Lang) -> String {
        format!("{}°  {}", self.temp, self.code.map(|c| condition(c, lang)).unwrap_or("")).trim_end().to_string()
    }

    pub fn range(&self) -> String {
        self.min.zip(self.max).map(|(lo, hi)| format!("{lo}° / {hi}°")).unwrap_or_default()
    }

    // Dos líneas cortas: la fuente de la isla es ancha y una sola no entra en el panel.
    pub fn detail(&self, lang: Lang) -> [String; 2] {
        let join = |p: [Option<String>; 2]| p.into_iter().flatten().collect::<Vec<_>>().join("  ·  ");
        [
            join([self.feels.map(|f| format!("{} {f}°", t_in(K::Feels, lang))), self.rain.map(|r| format!("{} {r}%", t_in(K::Rain, lang)))]),
            join([self.humidity.map(|h| format!("{} {h}%", t_in(K::Humidity, lang))), self.wind.map(|w| format!("{} {w} km/h", t_in(K::Wind, lang)))]),
        ]
    }

    pub fn url(&self) -> String {
        format!("https://wttr.in/{:.4},{:.4}", self.lat, self.lon)
    }
}

// La ubicación propia (`location` en user.json) manda; la caché de YASB es el respaldo de quien ya lo tenía.
pub fn pick_coords(configured: Option<(f64, f64)>, cached: Option<(f64, f64)>) -> Option<(f64, f64)> {
    configured.or(cached).filter(|(lat, lon)| (-90.0..=90.0).contains(lat) && (-180.0..=180.0).contains(lon))
}

fn yasb_coords() -> Option<(f64, f64)> {
    let p = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("YASB").join("weather.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(p).ok()?).ok()?;
    Some((v["weather"]["latitude"].as_f64()?, v["weather"]["longitude"].as_f64()?))
}

// Open-Meteo vía curl.exe (viene con Windows 11): sin dependencia HTTP ni clave; una consulta cada 30 min, cada minuto mientras falle.
pub fn spawn_weather(theme_path: PathBuf) {
    std::thread::spawn(move || loop {
        let configured = crate::theme::Theme::load(&theme_path).and_then(|t| t.location).map(|l| (l.lat, l.lon));
        let mut wait = 60;
        if let Some((lat, lon)) = pick_coords(configured, yasb_coords()) {
            let url = format!("https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code&daily=temperature_2m_max,temperature_2m_min,precipitation_probability_max&timezone=auto&forecast_days=1");
            let out = std::process::Command::new("curl.exe").args(["-s", "--max-time", "10", &url]).creation_flags(CREATE_NO_WINDOW).output();
            if let Some(w) = out.ok().and_then(|o| parse_weather(&String::from_utf8_lossy(&o.stdout), lat, lon)) {
                *WEATHER.lock().unwrap() = Some(w);
                wait = 1800;
            }
        }
        std::thread::sleep(Duration::from_secs(wait));
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

    const BODY: &str = r#"{"current":{"temperature_2m":14.5,"apparent_temperature":12.6,"relative_humidity_2m":60,"wind_speed_10m":11.7,"weather_code":2},"daily":{"temperature_2m_max":[19.2],"temperature_2m_min":[9.4],"precipitation_probability_max":[30]}}"#;

    #[test]
    fn parses_the_full_open_meteo_answer() {
        let w = parse_weather(BODY, -34.6, -58.4).unwrap();
        assert_eq!((w.temp, w.feels, w.humidity, w.wind, w.min, w.max, w.rain), (15, Some(13), Some(60), Some(12), Some(9), Some(19), Some(30)));
        assert_eq!(w.title(Lang::En), "15°  Partly cloudy");
        assert_eq!(w.title(Lang::Es), "15°  Algo nublado");
        assert_eq!(w.range(), "9° / 19°");
        assert_eq!(w.detail(Lang::En), ["Feels 13°  ·  Rain 30%".to_string(), "Humidity 60%  ·  Wind 12 km/h".to_string()]);
        assert_eq!(w.detail(Lang::Es), ["Sensación 13°  ·  Lluvia 30%".to_string(), "Humedad 60%  ·  Viento 12 km/h".to_string()]);
        assert_eq!(w.url(), "https://wttr.in/-34.6000,-58.4000");
    }

    #[test]
    fn tolerates_missing_optional_fields_and_rejects_garbage() {
        let w = parse_weather(r#"{"current":{"temperature_2m":3.0}}"#, 0.0, 0.0).unwrap();
        assert_eq!((w.title(Lang::En).as_str(), w.range().as_str(), w.detail(Lang::En)), ("3°", "", [String::new(), String::new()]));
        assert!(parse_weather("{}", 0.0, 0.0).is_none());
        assert!(parse_weather("no json", 0.0, 0.0).is_none());
    }

    #[test]
    fn own_location_beats_the_yasb_cache_and_is_range_checked() {
        assert_eq!(pick_coords(Some((1.0, 2.0)), Some((3.0, 4.0))), Some((1.0, 2.0)));
        assert_eq!(pick_coords(None, Some((3.0, 4.0))), Some((3.0, 4.0)));
        assert_eq!(pick_coords(None, None), None);
        assert_eq!(pick_coords(Some((95.0, 0.0)), None), None);
    }
}
