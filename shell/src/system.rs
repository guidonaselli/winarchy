// Estado del sistema que muestran la isla y el cluster del dock, muestreado fuera del hilo de UI:
// las llamadas a COM y a WinRT pueden bloquear y la UI solo lee la última foto.
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

// El volumen se lee 4 veces por segundo para que el OSD siga a las teclas; lo demás, una vez por segundo.
pub fn spawn(on_change: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        let mut n = 0u32;
        loop {
            let mut next = snapshot();
            next.volume = metrics::volume();
            if n % SLOW_EVERY == 0 {
                next.online = metrics::online();
                next.battery = metrics::battery();
                next.notifications = metrics::notification_count();
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
