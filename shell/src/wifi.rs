//! Lista y conexión Wi-Fi para la shell de sesión, donde el panel de red de Windows no existe.
use crate::i18n::{t, K};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows::core::{GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, HANDLE};
use windows::Win32::NetworkManagement::WiFi::*;
use windows::Win32::Security::Credentials::*;

const SHOWN: usize = 6;
// el menú espera esto como mucho: sin respuesta se abre sin redes
const SCAN_WAIT: Duration = Duration::from_millis(400);
const CONNECT_TRIES: u32 = 30;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Security {
    Open,
    Wpa,
    Wpa2,
    Wpa3,
    // empresarial u otro: solo se puede usar con un perfil ya guardado
    Other,
}

#[derive(Clone, Debug)]
pub struct Network {
    pub ssid: Vec<u8>,
    pub signal: u32,
    pub security: Security,
    pub tkip: bool,
    pub profile: Option<String>,
    pub connected: bool,
    interface: GUID,
}

pub enum Scan {
    Networks(Vec<Network>),
    // Windows 11 exige el permiso de ubicación para listar redes
    Denied,
    Unavailable,
}

impl Network {
    pub fn name(&self) -> String {
        String::from_utf8_lossy(&self.ssid).into_owned()
    }

    pub fn label(&self) -> String {
        let state = if self.connected {
            format!(" · {}", t(K::WifiConnected))
        } else if self.security == Security::Open {
            format!(" · {}", t(K::WifiOpen))
        } else {
            String::new()
        };
        format!("{} · {}%{state}", self.name(), self.signal)
    }
}

// Un renglón por SSID: gana el conectado o con perfil, con la mejor señal de sus puntos de acceso.
pub fn visible(mut raw: Vec<Network>) -> Vec<Network> {
    raw.retain(|n| !n.ssid.is_empty() && (n.security != Security::Other || n.profile.is_some()));
    raw.sort_by_key(|n| (!n.connected, n.profile.is_none(), std::cmp::Reverse(n.signal)));
    let mut out: Vec<Network> = Vec::new();
    for network in raw {
        match out.iter_mut().find(|kept| kept.ssid == network.ssid) {
            Some(kept) => kept.signal = kept.signal.max(network.signal),
            None => out.push(network),
        }
    }
    out.truncate(SHOWN);
    out
}

pub fn valid_passphrase(key: &str) -> bool {
    (8..=63).contains(&key.chars().count()) && !key.chars().any(char::is_control)
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// El SSID va en hex: puede no ser UTF-8.
pub fn profile_xml(network: &Network, key: &str) -> String {
    let name = xml(&network.name());
    let hex: String = network.ssid.iter().map(|b| format!("{b:02X}")).collect();
    let authentication = match network.security {
        Security::Open | Security::Other => "open",
        Security::Wpa => "WPAPSK",
        Security::Wpa2 => "WPA2PSK",
        Security::Wpa3 => "WPA3SAE",
    };
    // una red abierta no se vuelve a usar sola
    let (mode, encryption, shared) = if network.security == Security::Open {
        ("manual", "none", String::new())
    } else {
        (
            "auto",
            if network.tkip { "TKIP" } else { "AES" },
            format!("<sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>{}</keyMaterial></sharedKey>", xml(key)),
        )
    };
    format!(
        "<?xml version=\"1.0\"?><WLANProfile xmlns=\"http://www.microsoft.com/networking/WLAN/profile/v1\"><name>{name}</name>\
<SSIDConfig><SSID><hex>{hex}</hex><name>{name}</name></SSID></SSIDConfig><connectionType>ESS</connectionType><connectionMode>{mode}</connectionMode>\
<MSM><security><authEncryption><authentication>{authentication}</authentication><encryption>{encryption}</encryption><useOneX>false</useOneX></authEncryption>{shared}</security></MSM></WLANProfile>"
    )
}

struct Client(HANDLE);

impl Client {
    fn open() -> Option<Self> {
        let (mut version, mut handle) = (0, HANDLE::default());
        (unsafe { WlanOpenHandle(2, None, &mut version, &mut handle) } == 0).then_some(Self(handle))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        unsafe { WlanCloseHandle(self.0, None) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn from_wide(buffer: &[u16]) -> String {
    String::from_utf16_lossy(&buffer[..buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len())])
}

fn interfaces(client: &Client) -> Vec<GUID> {
    let mut interfaces = std::ptr::null_mut();
    if unsafe { WlanEnumInterfaces(client.0, None, &mut interfaces) } != 0 || interfaces.is_null() {
        return Vec::new();
    }
    unsafe {
        let list = &*interfaces;
        let ids = std::slice::from_raw_parts(list.InterfaceInfo.as_ptr(), list.dwNumberOfItems as usize).iter().map(|i| i.InterfaceGuid).collect();
        WlanFreeMemory(interfaces.cast());
        ids
    }
}

fn read(client: &Client) -> Scan {
    let ids = interfaces(client);
    if ids.is_empty() {
        return Scan::Unavailable;
    }
    let mut found = Vec::new();
    for interface in ids {
        let mut available = std::ptr::null_mut();
        let code = unsafe { WlanGetAvailableNetworkList(client.0, &interface, 0, None, &mut available) };
        if code == ERROR_ACCESS_DENIED.0 {
            return Scan::Denied;
        }
        if code != 0 || available.is_null() {
            continue;
        }
        unsafe {
            let list = &*available;
            for n in std::slice::from_raw_parts(list.Network.as_ptr(), list.dwNumberOfItems as usize) {
                let length = (n.dot11Ssid.uSSIDLength as usize).min(n.dot11Ssid.ucSSID.len());
                let security = match n.dot11DefaultAuthAlgorithm {
                    DOT11_AUTH_ALGO_80211_OPEN if !n.bSecurityEnabled.as_bool() => Security::Open,
                    DOT11_AUTH_ALGO_WPA_PSK => Security::Wpa,
                    DOT11_AUTH_ALGO_RSNA_PSK => Security::Wpa2,
                    DOT11_AUTH_ALGO_WPA3_SAE => Security::Wpa3,
                    _ => Security::Other,
                };
                found.push(Network {
                    ssid: n.dot11Ssid.ucSSID[..length].to_vec(),
                    signal: n.wlanSignalQuality.min(100),
                    security,
                    tkip: n.dot11DefaultCipherAlgorithm == DOT11_CIPHER_ALGO_TKIP,
                    profile: (n.dwFlags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE != 0).then(|| from_wide(&n.strProfileName)).filter(|p| !p.is_empty()),
                    connected: n.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED != 0,
                    interface,
                });
            }
            WlanFreeMemory(available.cast());
        }
    }
    Scan::Networks(visible(found))
}

pub fn scan() -> Scan {
    // con el servicio colgado no se acumulan hilos: uno a la vez
    static SCANNING: AtomicBool = AtomicBool::new(false);
    if SCANNING.swap(true, Ordering::Relaxed) {
        return Scan::Unavailable;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Some(client) = Client::open() {
            let _ = tx.send(read(&client));
            // la lista es la caché de Windows: se pide un barrido para la próxima apertura
            for interface in interfaces(&client) {
                unsafe { WlanScan(client.0, &interface, None, None, None) };
            }
        }
        SCANNING.store(false, Ordering::Relaxed);
    });
    rx.recv_timeout(SCAN_WAIT).unwrap_or(Scan::Unavailable)
}

// `None` si el usuario cancela. El diálogo es el de credenciales de Windows, con el SSID fijo como usuario.
fn ask_passphrase(name: &str) -> Option<String> {
    let (title, message, target) = (wide("Winarchy"), wide(t(K::WifiPassword)), wide(name));
    let info = CREDUI_INFOW {
        cbSize: std::mem::size_of::<CREDUI_INFOW>() as u32,
        pszMessageText: PCWSTR(message.as_ptr()),
        pszCaptionText: PCWSTR(title.as_ptr()),
        ..Default::default()
    };
    let mut user = [0u16; 128];
    for (slot, c) in user.iter_mut().zip(name.encode_utf16().take(127)) {
        *slot = c;
    }
    let mut password = [0u16; 128];
    let code = unsafe {
        CredUIPromptForCredentialsW(
            Some(&info),
            PCWSTR(target.as_ptr()),
            None,
            0,
            &mut user,
            &mut password,
            None,
            CREDUI_FLAGS_GENERIC_CREDENTIALS | CREDUI_FLAGS_ALWAYS_SHOW_UI | CREDUI_FLAGS_DO_NOT_PERSIST | CREDUI_FLAGS_KEEP_USERNAME,
        )
    };
    let key = (code.0 == 0).then(|| from_wide(&password));
    password.fill(0);
    key
}

fn start(client: &Client, network: &Network, profile: &str) -> bool {
    let name = wide(profile);
    let parameters = WLAN_CONNECTION_PARAMETERS {
        wlanConnectionMode: wlan_connection_mode_profile,
        strProfile: PCWSTR(name.as_ptr()),
        dot11BssType: dot11_BSS_type_infrastructure,
        ..Default::default()
    };
    unsafe { WlanConnect(client.0, &network.interface, &parameters, None) == 0 }
}

fn connected(client: &Client, ssid: &[u8]) -> bool {
    (0..CONNECT_TRIES).any(|_| {
        std::thread::sleep(Duration::from_millis(500));
        matches!(read(client), Scan::Networks(list) if list.iter().any(|n| n.connected && n.ssid == ssid))
    })
}

fn wipe(mut secret: String) {
    unsafe { secret.as_bytes_mut() }.fill(0);
}

// `false` si Windows rechaza el perfil; la clave no queda en memoria
fn save(client: &Client, network: &Network, key: &str, flags: u32, overwrite: bool) -> bool {
    let text = profile_xml(network, key);
    let mut profile = wide(&text);
    wipe(text);
    let mut reason = 0;
    let code = unsafe { WlanSetProfile(client.0, &network.interface, flags, PCWSTR(profile.as_ptr()), PCWSTR::null(), overwrite, None, &mut reason) };
    profile.fill(0);
    code == 0
}

fn stored(client: &Client, network: &Network, name: &str) -> Option<(Vec<u16>, u32)> {
    let name = wide(name);
    let (mut text, mut flags) = (PWSTR::null(), 0);
    if unsafe { WlanGetProfile(client.0, &network.interface, PCWSTR(name.as_ptr()), None, &mut text, Some(&mut flags), None) } != 0 {
        return None;
    }
    unsafe {
        let copy = text.as_wide().iter().copied().chain(Some(0)).collect();
        WlanFreeMemory(text.0.cast());
        // solo importa el bit de perfil de usuario al volver a escribirlo
        Some((copy, flags & 2))
    }
}

// La clave guardada puede haber cambiado en el router: se pide otra y, si tampoco conecta, vuelve el perfil anterior.
fn renew(client: &Client, network: &Network, profile: &str) -> Option<bool> {
    let renamed = Network { profile: None, ..network.clone() };
    // un perfil con otro nombre lo creó otra herramienta: no se pisa
    if profile != renamed.name() {
        return None;
    }
    let (previous, flags) = stored(client, network, profile)?;
    let key = ask_passphrase(&network.name())?;
    let saved = valid_passphrase(&key) && save(client, &renamed, &key, flags, true);
    wipe(key);
    if !saved {
        return Some(false);
    }
    let ok = start(client, network, profile) && connected(client, &network.ssid);
    if !ok {
        let mut reason = 0;
        unsafe { WlanSetProfile(client.0, &network.interface, flags, PCWSTR(previous.as_ptr()), PCWSTR::null(), true, None, &mut reason) };
    }
    Some(ok)
}

fn join(network: &Network) -> Option<bool> {
    let client = Client::open()?;
    if let Some(profile) = &network.profile {
        if start(&client, network, profile) && connected(&client, &network.ssid) {
            return Some(true);
        }
        let personal = matches!(network.security, Security::Wpa | Security::Wpa2 | Security::Wpa3);
        return Some(personal && renew(&client, network, profile) == Some(true));
    }
    let key = if network.security == Security::Open {
        String::new()
    } else {
        let key = ask_passphrase(&network.name())?;
        if !valid_passphrase(&key) {
            return Some(false);
        }
        key
    };
    // 2 = perfil del usuario: no pide administrador
    let saved = save(&client, network, &key, 2, false);
    wipe(key);
    if !saved {
        return Some(false);
    }
    let name = network.name();
    let ok = start(&client, network, &name) && connected(&client, &network.ssid);
    if !ok {
        // una clave errónea no debe quedar guardada: la próxima vez se vuelve a pedir
        let name = wide(&name);
        unsafe { WlanDeleteProfile(client.0, &network.interface, PCWSTR(name.as_ptr()), None) };
    }
    Some(ok)
}

pub fn connect(network: Network) {
    static BUSY: AtomicBool = AtomicBool::new(false);
    if network.connected || BUSY.swap(true, Ordering::Relaxed) {
        return;
    }
    std::thread::spawn(move || {
        if let Some(ok) = join(&network) {
            let state = t(if ok { K::WifiConnected } else { K::WifiFailed });
            crate::notify(&format!("{} · {state}", network.name()));
        }
        BUSY.store(false, Ordering::Relaxed);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(ssid: &str, signal: u32, security: Security) -> Network {
        Network { ssid: ssid.as_bytes().to_vec(), signal, security, tkip: false, profile: None, connected: false, interface: GUID::zeroed() }
    }

    #[test]
    fn one_row_per_ssid_connected_then_saved_then_signal() {
        let mut saved = network("saved", 20, Security::Wpa2);
        saved.profile = Some("saved".into());
        let mut current = network("current", 40, Security::Wpa2);
        current.connected = true;
        let list = visible(vec![
            network("far", 30, Security::Wpa2),
            network("", 99, Security::Open),
            network("corp", 99, Security::Other),
            network("near", 80, Security::Open),
            network("far", 60, Security::Wpa2),
            saved,
            current,
        ]);
        let rows: Vec<_> = list.iter().map(|n| (n.name(), n.signal)).collect();
        assert_eq!(rows, [("current".into(), 40), ("saved".into(), 20), ("near".into(), 80), ("far".into(), 60)]);
    }

    #[test]
    fn profile_escapes_names_and_keys_and_omits_the_key_for_open_networks() {
        let secured = profile_xml(&network("a<&b", 50, Security::Wpa3), "p\"ss<word>&");
        assert!(secured.contains("<name>a&lt;&amp;b</name>") && secured.contains("<hex>613C2662</hex>"));
        assert!(secured.contains("<authentication>WPA3SAE</authentication><encryption>AES</encryption>"));
        assert!(secured.contains("<keyMaterial>p&quot;ss&lt;word&gt;&amp;</keyMaterial>"));
        let open = profile_xml(&network("cafe", 50, Security::Open), "");
        assert!(open.contains("<authentication>open</authentication><encryption>none</encryption>") && !open.contains("sharedKey"));
        assert!(open.contains("<connectionMode>manual</connectionMode>") && secured.contains("<connectionMode>auto</connectionMode>"));
    }

    #[test]
    #[ignore = "lee las redes de la máquina"]
    fn scans_this_machine_read_only() {
        let started = std::time::Instant::now();
        match scan() {
            Scan::Networks(list) => println!("{} networks, {} saved, in {:?}", list.len(), list.iter().filter(|n| n.profile.is_some()).count(), started.elapsed()),
            Scan::Denied => println!("denied in {:?}", started.elapsed()),
            Scan::Unavailable => println!("unavailable in {:?}", started.elapsed()),
        }
    }

    #[test]
    fn passphrases_follow_the_wpa_length_limits() {
        assert!(!valid_passphrase("1234567") && valid_passphrase("12345678") && valid_passphrase(&"x".repeat(63)));
        assert!(!valid_passphrase(&"x".repeat(64)) && !valid_passphrase("12345678\n"));
    }
}
