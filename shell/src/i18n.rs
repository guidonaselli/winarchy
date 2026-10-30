// Todo texto visible de la shell vive en esta tabla, en inglés y español: agregar un idioma o un texto sin
// completar ambas columnas no compila. El idioma sale de WINARCHY_LANG o del regional de Windows (como el asistente) y se lee al arrancar.
use std::sync::OnceLock;
use windows::Win32::Globalization::GetUserDefaultLocaleName;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Es,
}

macro_rules! messages {
    ($($key:ident => ($en:expr, $es:expr)),* $(,)?) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum K { $($key),* }

        impl K {
            #[cfg(test)]
            const ALL: &'static [K] = &[$(K::$key),*];

            fn text(self, lang: Lang) -> &'static str {
                match (self, lang) { $((K::$key, Lang::En) => $en, (K::$key, Lang::Es) => $es),* }
            }
        }
    };
}

messages! {
    Listening => ("Listening", "Escuchando"),
    Thinking => ("Thinking", "Pensando"),
    Speaking => ("Speaking", "Hablando"),
    Error => ("Error", "Error"),
    Cpu => ("CPU", "CPU"),
    Ram => ("RAM", "RAM"),
    Gpu => ("GPU", "GPU"),
    Vol => ("VOL", "VOL"),
    MuteOsd => ("MUTE", "SILENCIO"),
    MuteValue => ("mute", "silencio"),
    Battery => ("BAT", "BAT"),
    Notifications => ("NOTIF", "NOTIF"),
    NetOff => ("NET off", "SIN RED"),
    Workspace => ("WS", "ESCRITORIO"),
    Layout => ("LAYOUT", "DISEÑO"),
    FlagGame => ("GAME", "JUEGO"),
    FlagAwake => ("AWAKE", "DESPIERTO"),
    FlagUpdate => ("UPDATE", "ACTUALIZAR"),
    Feels => ("Feels", "Sensación"),
    Rain => ("Rain", "Lluvia"),
    Humidity => ("Humidity", "Humedad"),
    Wind => ("Wind", "Viento"),
    CondClear => ("Clear", "Despejado"),
    CondPartlyCloudy => ("Partly cloudy", "Algo nublado"),
    CondOvercast => ("Overcast", "Nublado"),
    CondFog => ("Fog", "Niebla"),
    CondDrizzle => ("Drizzle", "Llovizna"),
    CondRain => ("Rain", "Lluvia"),
    CondSnow => ("Snow", "Nieve"),
    CondShowers => ("Showers", "Chubascos"),
    CondSnowShowers => ("Snow showers", "Chubascos de nieve"),
    CondThunderstorm => ("Thunderstorm", "Tormenta"),
}

// `es` solo si el idioma pedido (o el regional de Windows) es español; cualquier otro cae en inglés.
pub fn lang_of(locale: &str) -> Lang {
    if locale.to_lowercase().starts_with("es") { Lang::Es } else { Lang::En }
}

fn system_locale() -> String {
    let mut b = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(&mut b) };
    String::from_utf16_lossy(&b[..(n.max(1) as usize - 1)])
}

pub fn lang() -> Lang {
    static LANG: OnceLock<Lang> = OnceLock::new();
    *LANG.get_or_init(|| lang_of(&std::env::var("WINARCHY_LANG").unwrap_or_else(|_| system_locale())))
}

pub fn t_in(k: K, lang: Lang) -> &'static str {
    k.text(lang)
}

pub fn t(k: K) -> &'static str {
    k.text(lang())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_only_for_spanish_locales() {
        assert_eq!(lang_of("es-AR"), Lang::Es);
        assert_eq!(lang_of("ES"), Lang::Es);
        assert_eq!(lang_of("en-US"), Lang::En);
        assert_eq!(lang_of("pt-BR"), Lang::En);
        assert_eq!(lang_of(""), Lang::En);
    }

    #[test]
    fn every_text_exists_in_both_languages() {
        for k in K::ALL {
            assert!(!t_in(*k, Lang::En).is_empty() && !t_in(*k, Lang::Es).is_empty(), "{k:?}");
        }
        assert_eq!(t_in(K::Speaking, Lang::Es), "Hablando");
        assert_eq!(t_in(K::Speaking, Lang::En), "Speaking");
    }
}
