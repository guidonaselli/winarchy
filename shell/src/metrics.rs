// shortcut: sin consumidor hasta decidir dónde se muestran los widgets (4.4), quitar el allow al conectarlo.
#![allow(dead_code)]
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

#[derive(Default)]
pub struct Sampler {
    prev: Option<(u64, u64, u64)>,
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

pub fn ram_percent() -> Option<u32> {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe { GlobalMemoryStatusEx(&mut m).ok()? };
    Some(m.dwMemoryLoad)
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
    fn ram_is_a_percentage() {
        assert!(ram_percent().unwrap() <= 100);
    }
}
