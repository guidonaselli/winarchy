// Estado del sistema que muestran la isla y el cluster del dock, muestreado fuera del hilo de UI:
// las llamadas a COM y a WinRT pueden bloquear y la UI solo lee la última foto.
use crate::i18n::{t, K};
use crate::metrics;
use std::sync::Mutex;

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Snapshot {
    pub volume: Option<(u32, bool)>,
    pub online: Option<bool>,
    pub battery: Option<(u8, bool)>,
    pub notifications: Option<usize>,
}

static CURRENT: Mutex<Snapshot> = Mutex::new(Snapshot { volume: None, online: None, battery: None, notifications: None });

pub fn snapshot() -> Snapshot {
    *CURRENT.lock().unwrap()
}

const VOLUME_MS: u64 = 250;
const SLOW_EVERY: u32 = 4;
// Las notificaciones vencen solas, sin escribir en la base: se recuentan igual cada 30 s.
const EXPIRY_EVERY: u32 = 120;

// avisa una vez al cruzar cada umbral; enchufar el cargador rearma los dos
fn battery_alert(battery: Option<(u8, bool)>, (low, critical): (u8, u8), warned: &mut u8) -> Option<u8> {
    let Some((pct, false)) = battery else {
        *warned = 0;
        return None;
    };
    let level = if pct <= critical { 2 } else { u8::from(pct <= low) };
    (level > *warned).then(|| {
        *warned = level;
        pct
    })
}

// El volumen se lee 4 veces por segundo para que el OSD siga a las teclas; lo demás, una vez por segundo.
pub fn spawn(on_change: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        let mut n = 0u32;
        let mut endpoint = None;
        let mut toasts_stamp = None;
        let mut warned = 0u8;
        loop {
            let mut next = snapshot();
            // el dispositivo por defecto puede cambiar: se vuelve a pedir una vez por segundo
            if n % SLOW_EVERY == 0 {
                endpoint = metrics::endpoint();
            }
            next.volume = endpoint.as_ref().and_then(metrics::read_volume);
            if n % SLOW_EVERY == 0 {
                next.online = metrics::online();
                next.battery = metrics::battery();
                // con Explorer el aviso ya lo da Windows
                if crate::HOST.get().is_some_and(|h| h.session_shell) {
                    if let Some(pct) = battery_alert(next.battery, metrics::battery_levels(), &mut warned) {
                        crate::notify(&format!("{} · {pct}%", t(K::LowBattery)));
                    }
                }
                // contar cuesta ~23 ms de CPU (COM): solo cuando la base de notificaciones cambió
                let stamp = Some(crate::wpn::stamp());
                if n % EXPIRY_EVERY == 0 || stamp != toasts_stamp {
                    toasts_stamp = stamp;
                    next.notifications = metrics::notification_count();
                }
            }
            n = n.wrapping_add(1);
            let changed = std::mem::replace(&mut *CURRENT.lock().unwrap(), next) != next;
            if changed {
                on_change();
            }
            std::thread::sleep(std::time::Duration::from_millis(VOLUME_MS));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_battery_warns_once_per_threshold_and_rearms_on_power() {
        let mut warned = 0;
        let mut at = |b| battery_alert(b, (10, 5), &mut warned);
        assert_eq!(at(None), None);
        assert_eq!(at(Some((40, false))), None);
        assert_eq!(at(Some((10, false))), Some(10));
        assert_eq!(at(Some((9, false))), None);
        assert_eq!(at(Some((11, false))), None);
        assert_eq!(at(Some((10, false))), None);
        assert_eq!(at(Some((5, false))), Some(5));
        assert_eq!(at(Some((4, false))), None);
        assert_eq!(at(Some((4, true))), None);
        assert_eq!(at(Some((4, false))), Some(4));
    }
}
