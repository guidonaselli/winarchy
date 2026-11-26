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
    Bri => ("BRI", "BRI"),
    MuteOsd => ("MUTE", "SILENCIO"),
    MuteValue => ("mute", "silencio"),
    Battery => ("BAT", "BAT"),
    Notifications => ("NOTIF", "NOTIF"),
    NotificationDismiss => ("Dismiss notification", "Descartar notificación"),
    MenuOpen => ("Open", "Abrir"),
    MenuNewWindow => ("New window", "Nueva ventana"),
    MenuPin => ("Pin to dock", "Anclar al dock"),
    MenuUnpin => ("Unpin from dock", "Desanclar del dock"),
    MenuClose => ("Close window", "Cerrar ventana"),
    MenuCloseAll => ("Close all windows", "Cerrar todas las ventanas"),
    StartMenu => ("Start · right-click: Winarchy menu", "Inicio · clic derecho: menú de Winarchy"),
    Start => ("Start", "Inicio"),
    StartSearch => ("Type to search apps", "Escribí para buscar apps"),
    StartPinned => ("Pinned", "Anclado"),
    StartAllApps => ("All apps  >", "Todas las apps  >"),
    StartBack => ("<  Pinned", "<  Anclado"),
    StartNoResults => ("No apps found", "No hay apps"),
    StartPlacesHint => ("Drop folders or files on the logo to pin them here", "Soltá carpetas o archivos sobre el logo para anclarlos acá"),
    StartPin => ("Pin to Start", "Anclar a Inicio"),
    StartUnpin => ("Unpin from Start", "Desanclar de Inicio"),
    PowerLock => ("Lock", "Bloquear"),
    PowerSleep => ("Sleep", "Suspender"),
    PowerShutdown => ("Shut down", "Apagar"),
    PowerRestart => ("Restart", "Reiniciar"),
    PowerSignOut => ("Sign out", "Cerrar sesión"),
    MoreApps => ("More applications", "Más aplicaciones"),
    Applications => ("Applications", "Aplicaciones"),
    SystemTray => ("System tray", "Bandeja del sistema"),
    PinToBar => ("Show in the bar", "Mostrar en la barra"),
    UnpinFromBar => ("Remove from the bar", "Quitar de la barra"),
    ShowDesktop => ("Show desktop", "Mostrar escritorio"),
    MenuSoundSettings => ("Sound settings", "Configuración de sonido"),
    MenuVolumeMixer => ("Open volume mixer", "Abrir mezclador de volumen"),
    MenuMute => ("Mute / unmute", "Silenciar / activar sonido"),
    MenuNetworkSettings => ("Network & Internet settings", "Configuración de red e Internet"),
    MenuWifiSettings => ("Wi-Fi settings", "Configuración de Wi-Fi"),
    MenuBluetoothSettings => ("Bluetooth & devices", "Bluetooth y dispositivos"),
    MenuDisplaySettings => ("Display & brightness", "Pantalla y brillo"),
    MenuNetworkConnections => ("Network connections", "Conexiones de red"),
    MenuPowerSettings => ("Power & sleep settings", "Configuración de energía y suspensión"),
    MenuDateTime => ("Adjust date and time", "Ajustar fecha y hora"),
    MenuNotificationSettings => ("Notification settings", "Configuración de notificaciones"),
    BarFull => ("The bar is full", "La barra está llena"),
    NetOff => ("NET off", "SIN RED"),
    Online => ("Online", "Conectado"),
    Offline => ("Offline", "Sin conexión"),
    Charging => ("charging", "cargando"),
    VolumeTitle => ("Volume", "Volumen"),
    QuickSettings => ("Quick settings", "Configuración rápida"),
    DateTime => ("Date and time", "Fecha y hora"),
    Workspace => ("WS", "ESCRITORIO"),
    Layout => ("LAYOUT", "DISEÑO"),
    ModeTiling => ("TILING", "MOSAICO"),
    ModeScrolling => ("SCROLL", "DESLIZAR"),
    ModeFloating => ("FLOAT", "LIBRE"),
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
    NoEvents => ("No events", "Sin eventos"),
    RecentNotifications => ("Notifications", "Notificaciones"),
    AllDay => ("All day", "Todo el día"),
    NoCalendar => ("Add a calendar URL in user.json", "Agregá la URL del calendario en user.json"),
    More => ("more", "más"),
    Now => ("now", "ahora"),
}

pub const MONTHS: [[&str; 12]; 2] = [
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
    ["Enero", "Febrero", "Marzo", "Abril", "Mayo", "Junio", "Julio", "Agosto", "Septiembre", "Octubre", "Noviembre", "Diciembre"],
];

// Iniciales de lunes a domingo.
pub const WEEKDAYS: [[&str; 7]; 2] = [["M", "T", "W", "T", "F", "S", "S"], ["L", "M", "M", "J", "V", "S", "D"]];

pub fn month_name(m: u32, lang: Lang) -> &'static str {
    MONTHS[lang as usize][m as usize - 1]
}

pub fn weekday_initial(wd: usize, lang: Lang) -> &'static str {
    WEEKDAYS[lang as usize][wd]
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
    fn brand_button_names_start_and_the_winarchy_context_menu_in_both_languages() {
        assert_eq!(t_in(K::StartMenu, Lang::En), "Start · right-click: Winarchy menu");
        assert_eq!(t_in(K::StartMenu, Lang::Es), "Inicio · clic derecho: menú de Winarchy");
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
