// Fechas civiles sin dependencias (algoritmos de Howard Hinnant) y la grilla del mes del panel del reloj.
// Los instantes son "minutos locales desde 1970-01-01": naive, sin zona.

use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};

pub const MIN_DAY: i64 = 1440;

pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m as i64 + 9) % 12) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

// 0 = lunes.
pub fn weekday(days: i64) -> u32 {
    (days + 3).rem_euclid(7) as u32
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    (days_from_civil(if m == 12 { y + 1 } else { y }, if m == 12 { 1 } else { m + 1 }, 1) - days_from_civil(y, m, 1)) as u32
}

// n-ésimo día de la semana `wd` del mes (n = -1 es el último); None si el mes no lo tiene.
pub fn nth_weekday(y: i64, m: u32, n: i32, wd: u32) -> Option<u32> {
    let dim = days_in_month(y, m) as i32;
    let day = if n > 0 {
        1 + ((wd + 7 - weekday(days_from_civil(y, m, 1))) % 7) as i32 + 7 * (n - 1)
    } else {
        dim - ((weekday(days_from_civil(y, m, dim as u32)) + 7 - wd) % 7) as i32 + 7 * (n + 1)
    };
    (1..=dim).contains(&day).then_some(day as u32)
}

pub fn shift_month(y: i64, m: u32, delta: i32) -> (i64, u32) {
    let t = y * 12 + (m as i64 - 1) + delta as i64;
    (t.div_euclid(12), (t.rem_euclid(12) + 1) as u32)
}

// Filas (semanas) y celdas con el día del mes; lunes primero.
pub fn month_grid(y: i64, m: u32) -> (usize, Vec<Option<u32>>) {
    let off = weekday(days_from_civil(y, m, 1)) as usize;
    let dim = days_in_month(y, m) as usize;
    let rows = (off + dim).div_ceil(7);
    (rows, (0..rows * 7).map(|i| i.checked_sub(off).filter(|d| *d < dim).map(|d| d as u32 + 1)).collect())
}

// Día local de hoy (días desde 1970) y minuto del día.
pub fn now_local() -> (i64, u32) {
    let t = unsafe { GetLocalTime() };
    (days_from_civil(t.wYear as i64, t.wMonth as u32, t.wDay as u32), t.wHour as u32 * 60 + t.wMinute as u32)
}

// Desfase actual local − UTC en minutos (sin tabla de zonas: el del momento de consultar).
pub fn utc_offset_min() -> i64 {
    let (l, u) = unsafe { (GetLocalTime(), GetSystemTime()) };
    let min = |t: windows::Win32::Foundation::SYSTEMTIME| days_from_civil(t.wYear as i64, t.wMonth as u32, t.wDay as u32) * MIN_DAY + t.wHour as i64 * 60 + t.wMinute as i64;
    min(l) - min(u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_round_trip_and_weekdays_match_the_calendar() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(days_from_civil(2026, 11, 8)), (2026, 11, 8));
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(weekday(days_from_civil(2026, 11, 8)), 6, "8 nov 2026 es domingo");
        assert_eq!(weekday(0), 3, "1 ene 1970 fue jueves");
        assert_eq!((days_in_month(2024, 2), days_in_month(2026, 2), days_in_month(2026, 12)), (29, 28, 31));
    }

    #[test]
    fn grid_starts_on_monday_and_covers_the_month() {
        let (rows, cells) = month_grid(2026, 11);
        assert_eq!((rows, cells.len()), (6, 42));
        assert_eq!(cells[6], Some(1), "1 nov 2026 es domingo: última columna de la primera fila");
        assert!(cells[..6].iter().all(Option::is_none));
        assert_eq!(cells.iter().flatten().count(), 30);
        assert_eq!(month_grid(2027, 2).0, 4, "feb 2027 empieza lunes y tiene 28 días");
        assert_eq!(month_grid(2026, 8).0, 6);
    }

    #[test]
    fn nth_weekday_counts_from_either_end() {
        assert_eq!(nth_weekday(2026, 11, 2, 1), Some(10), "segundo martes de nov 2026");
        assert_eq!(nth_weekday(2026, 11, -1, 4), Some(27), "último viernes");
        assert_eq!(nth_weekday(2026, 2, 5, 0), None);
    }

    #[test]
    fn months_shift_across_years() {
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 5, 0), (2026, 5));
    }
}
