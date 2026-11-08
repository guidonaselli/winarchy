// Relojes de otras ciudades para el panel del reloj: la hora sale de las zonas de Windows, sin red.

use crate::calendar::{days_from_civil, now_local};
use crate::theme::ClockCfg;
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::GetSystemTime;
use windows::Win32::System::Time::{SystemTimeToTzSpecificLocalTimeEx, DYNAMIC_TIME_ZONE_INFORMATION};

// (días desde 1970, minutos del día) ahora en la zona `tz` (nombre de Windows, p. ej. "Tokyo Standard Time").
fn now_in(tz: &str) -> Option<(i64, u32)> {
    let mut info = DYNAMIC_TIME_ZONE_INFORMATION::default();
    let name: Vec<u16> = tz.encode_utf16().collect();
    if name.is_empty() || name.len() >= info.TimeZoneKeyName.len() {
        return None;
    }
    info.TimeZoneKeyName[..name.len()].copy_from_slice(&name);
    let mut local = SYSTEMTIME::default();
    unsafe { SystemTimeToTzSpecificLocalTimeEx(Some(&info), &GetSystemTime(), &mut local).ok()? };
    Some((days_from_civil(local.wYear as i64, local.wMonth as u32, local.wDay as u32), local.wHour as u32 * 60 + local.wMinute as u32))
}

// "NYC 09:41 · TYO 23:41 +1d": el sufijo marca que allá es otro día que acá.
pub fn line(clocks: &[ClockCfg]) -> String {
    let today = now_local().0;
    clocks
        .iter()
        .map(|c| match now_in(&c.tz) {
            Some((d, m)) => format!("{} {:02}:{:02}{}", c.city, m / 60, m % 60, match d - today { 1 => " +1d", -1 => " -1d", _ => "" }),
            None => format!("{} ?", c.city),
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_and_tokyo_differ_by_nine_hours_and_a_typo_is_flagged() {
        let min = |tz| now_in(tz).map(|(d, m)| d * 1440 + m as i64);
        assert_eq!(min("Tokyo Standard Time").unwrap() - min("UTC").unwrap(), 540);
        assert!(now_in("Nope Standard Time").is_none() || min("Nope Standard Time") != min("Tokyo Standard Time"));
    }

    #[test]
    fn line_lists_every_city_with_a_marker_for_unknown_zones() {
        let c = |city: &str, tz: &str| ClockCfg { city: city.into(), tz: tz.into() };
        let l = line(&[c("TYO", "Tokyo Standard Time"), c("???", "")]);
        assert!(l.starts_with("TYO ") && l.contains(" · ") && l.ends_with("??? ?"), "{l}");
    }
}
