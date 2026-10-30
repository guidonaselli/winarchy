use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::Networking::NetworkListManager::{INetworkListManager, NetworkListManager, NLM_CONNECTIVITY_IPV4_INTERNET, NLM_CONNECTIVITY_IPV6_INTERNET};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::core::{w, PCWSTR};
use windows::Win32::System::Performance::{PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

#[derive(Default)]
pub struct Sampler {
    prev: Option<(u64, u64, u64)>,
    gpu: Option<(isize, isize)>,
}

fn ticks(f: FILETIME) -> u64 {
    (f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64
}

// Porcentaje de CPU entre dos llamadas; la primera no tiene referencia y devuelve None.
pub fn cpu_percent(prev: (u64, u64, u64), now: (u64, u64, u64)) -> f32 {
    let (idle, total) = (now.0 - prev.0, (now.1 - prev.1) + (now.2 - prev.2));
    if total == 0 {
        return 0.0;
    }
    (100.0 * (1.0 - idle as f64 / total as f64)).clamp(0.0, 100.0) as f32
}

impl Sampler {
    pub fn cpu(&mut self) -> Option<f32> {
        let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()? };
        // kernel incluye idle: total = kernel + user.
        let now = (ticks(idle), ticks(kernel), ticks(user));
        let r = self.prev.map(|p| cpu_percent(p, now));
        self.prev = Some(now);
        r
    }
}

impl Sampler {
    // Uso del motor 3D de todas las GPU (contador PDH "GPU Engine"); None si no hay contador o es la primera muestra.
    pub fn gpu(&mut self) -> Option<f32> {
        unsafe {
            if self.gpu.is_none() {
                let (mut q, mut c) = (0isize, 0isize);
                if PdhOpenQueryW(PCWSTR::null(), 0, &mut q) != 0 || PdhAddEnglishCounterW(q, w!("\\GPU Engine(*)\\Utilization Percentage"), 0, &mut c) != 0 {
                    return None;
                }
                PdhCollectQueryData(q);
                self.gpu = Some((q, c));
                return None;
            }
            let (q, c) = self.gpu?;
            PdhCollectQueryData(q);
            let (mut size, mut count) = (0u32, 0u32);
            PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, None);
            if size == 0 {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            if PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items)) != 0 {
                return None;
            }
            let total: f64 = std::slice::from_raw_parts(items, count as usize)
                .iter()
                .filter(|i| i.szName.to_string().is_ok_and(|n| n.contains("engtype_3D")))
                .map(|i| i.FmtValue.Anonymous.doubleValue)
                .sum();
            Some(total.clamp(0.0, 100.0) as f32)
        }
    }
}

pub fn ram_percent() -> Option<u32> {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe { GlobalMemoryStatusEx(&mut m).ok()? };
    Some(m.dwMemoryLoad)
}

// None si no hay batería (escritorio).
pub fn battery() -> Option<(u8, bool)> {
    let mut p = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut p).ok()? };
    (p.BatteryFlag & 128 == 0 && p.BatteryLifePercent <= 100).then_some((p.BatteryLifePercent, p.ACLineStatus == 1))
}

fn endpoint() -> Option<IAudioEndpointVolume> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        dev.Activate(CLSCTX_ALL, None).ok()
    }
}

// Requiere COM inicializado en el hilo que llama (igual que `adjust_volume` y `toggle_mute`).
pub fn volume() -> Option<(u32, bool)> {
    let vol = endpoint()?;
    unsafe { Some(((vol.GetMasterVolumeLevelScalar().ok()? * 100.0).round() as u32, vol.GetMute().ok()?.as_bool())) }
}

pub fn adjust_volume(delta_percent: i32) -> Option<()> {
    let vol = endpoint()?;
    unsafe {
        let level = (vol.GetMasterVolumeLevelScalar().ok()? + delta_percent as f32 / 100.0).clamp(0.0, 1.0);
        vol.SetMasterVolumeLevelScalar(level, std::ptr::null()).ok()
    }
}

pub fn set_volume(percent: u32) -> Option<()> {
    let vol = endpoint()?;
    unsafe {
        vol.SetMute(windows::Win32::Foundation::BOOL::from(false), std::ptr::null()).ok()?;
        vol.SetMasterVolumeLevelScalar(percent.min(100) as f32 / 100.0, std::ptr::null()).ok()
    }
}

pub fn toggle_mute() -> Option<()> {
    let vol = endpoint()?;
    unsafe {
        let muted = vol.GetMute().ok()?.as_bool();
        vol.SetMute(windows::Win32::Foundation::BOOL::from(!muted), std::ptr::null()).ok()
    }
}

pub fn online() -> Option<bool> {
    unsafe {
        let nlm: INetworkListManager = CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL).ok()?;
        let c = nlm.GetConnectivity().ok()?.0;
        Some(c & (NLM_CONNECTIVITY_IPV4_INTERNET.0 | NLM_CONNECTIVITY_IPV6_INTERNET.0) != 0)
    }
}

// Flags que escribe Winarchy en state\ (game mode, stay awake, update); un archivo presente = activo.
pub fn flags(state_dir: &std::path::Path) -> Vec<crate::i18n::K> {
    use crate::i18n::K;
    [("game-mode.flag", K::FlagGame), ("stay-awake.flag", K::FlagAwake), ("update-available.flag", K::FlagUpdate)]
        .into_iter()
        .filter(|(f, _)| state_dir.join(f).exists())
        .map(|(_, l)| l)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_is_busy_share_of_total() {
        assert_eq!(cpu_percent((0, 0, 0), (50, 60, 40)), 50.0);
        assert_eq!(cpu_percent((10, 10, 10), (10, 10, 10)), 0.0);
    }

    #[test]
    fn flags_follow_files() {
        let d = std::env::temp_dir().join("ws-flags-test");
        let _ = std::fs::create_dir_all(&d);
        let _ = std::fs::remove_file(d.join("stay-awake.flag"));
        assert!(!flags(&d).contains(&crate::i18n::K::FlagAwake));
        std::fs::write(d.join("stay-awake.flag"), "").unwrap();
        assert!(flags(&d).contains(&crate::i18n::K::FlagAwake));
    }

    #[test]
    fn ram_is_a_percentage() {
        assert!(ram_percent().unwrap() <= 100);
    }
}

// Cantidad de toasts en el centro de notificaciones; None si el acceso no fue concedido.
pub fn notification_count() -> Option<usize> {
    use windows::UI::Notifications::Management::{UserNotificationListener, UserNotificationListenerAccessStatus};
    use windows::UI::Notifications::NotificationKinds;
    let l = UserNotificationListener::Current().ok()?;
    if l.GetAccessStatus().ok()? != UserNotificationListenerAccessStatus::Allowed {
        return None;
    }
    Some(l.GetNotificationsAsync(NotificationKinds::Toast).ok()?.get().ok()?.Size().ok()? as usize)
}
