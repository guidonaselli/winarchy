// Feed ICS de solo lectura (p. ej. la "dirección secreta en formato iCal" de Google Calendar).
// shortcut: las fechas con TZID o flotantes se toman como hora local y el desfase de UTC es el actual (sin tabla de zonas);
// sirve para un calendario propio en una sola zona, mejorar si hay eventos en otras zonas.

use crate::calendar::{civil_from_days, days_from_civil, days_in_month, nth_weekday, weekday, MIN_DAY};
use std::collections::BTreeSet;
use std::os::windows::process::CommandExt;
use std::sync::Mutex;
use std::time::Duration;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_STEPS: i64 = 4000;

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub title: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Debug)]
struct Rule {
    freq: Freq,
    interval: i64,
    count: Option<u32>,
    until: Option<i64>,
    // (ordinal, día de la semana con lunes = 0); ordinal 0 = sin ordinal.
    byday: Vec<(i32, u32)>,
}

#[derive(Clone, Debug)]
struct Raw {
    uid: String,
    start: i64,
    end: i64,
    all_day: bool,
    title: String,
    rule: Option<Rule>,
    exdates: Vec<i64>,
    is_override: bool,
}

#[derive(Clone, Default, Debug)]
pub struct Feed {
    raw: Vec<Raw>,
    // Instancias de una serie que otro VEVENT (RECURRENCE-ID) movió o canceló.
    overridden: BTreeSet<(String, i64)>,
}

pub static FEED: Mutex<Feed> = Mutex::new(Feed { raw: Vec::new(), overridden: BTreeSet::new() });

fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        match (line.strip_prefix([' ', '\t']), out.last_mut()) {
            (Some(rest), Some(prev)) => prev.push_str(rest),
            _ => out.push(line.to_string()),
        }
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("\\n", " ").replace("\\N", " ").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

// "20261108T130000Z" / "20261108T130000" / "20261108" → (minutos locales, solo fecha).
fn parse_dt(value: &str, utc_offset: i64) -> Option<(i64, bool)> {
    let v = value.trim();
    let num = |a: usize, b: usize| v.get(a..b)?.parse::<u32>().ok();
    let day = days_from_civil(v.get(0..4)?.parse().ok()?, num(4, 6)?, num(6, 8)?);
    if v.len() == 8 {
        return Some((day * MIN_DAY, true));
    }
    let local = day * MIN_DAY + num(9, 11)? as i64 * 60 + num(11, 13)? as i64;
    Some((if v.ends_with('Z') { local + utc_offset } else { local }, false))
}

fn parse_rule(value: &str, utc_offset: i64) -> Option<Rule> {
    let mut rule = Rule { freq: Freq::Daily, interval: 1, count: None, until: None, byday: Vec::new() };
    let mut has_freq = false;
    for part in value.split(';') {
        let (k, v) = part.split_once('=')?;
        match k {
            "FREQ" => {
                rule.freq = match v {
                    "DAILY" => Freq::Daily,
                    "WEEKLY" => Freq::Weekly,
                    "MONTHLY" => Freq::Monthly,
                    "YEARLY" => Freq::Yearly,
                    _ => return None,
                };
                has_freq = true;
            }
            "INTERVAL" => rule.interval = v.parse::<i64>().ok()?.max(1),
            "COUNT" => rule.count = v.parse().ok(),
            "UNTIL" => rule.until = parse_dt(v, utc_offset).map(|(m, all_day)| if all_day { m + MIN_DAY - 1 } else { m }),
            "BYDAY" => {
                rule.byday = v
                    .split(',')
                    .filter_map(|d| {
                        let (n, code) = d.split_at(d.len().checked_sub(2).filter(|i| d.is_char_boundary(*i))?);
                        let wd = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].iter().position(|c| *c == code)? as u32;
                        Some((if n.is_empty() { 0 } else { n.parse().ok()? }, wd))
                    })
                    .collect();
            }
            "BYMONTH" | "BYSETPOS" | "BYMONTHDAY" | "BYYEARDAY" | "BYWEEKNO" => return None,
            _ => {}
        }
    }
    has_freq.then_some(rule)
}

impl Feed {
    pub fn parse(text: &str, utc_offset: i64) -> Feed {
        let mut feed = Feed::default();
        let mut cur: Option<(Raw, bool, Option<i64>)> = None;
        for line in unfold(text) {
            if line == "BEGIN:VEVENT" {
                cur = Some((Raw { uid: String::new(), start: 0, end: 0, all_day: false, title: String::new(), rule: None, exdates: Vec::new(), is_override: false }, false, None));
                continue;
            }
            if line == "END:VEVENT" {
                if let Some((mut raw, cancelled, rid)) = cur.take() {
                    if let Some(rid) = rid {
                        feed.overridden.insert((raw.uid.clone(), rid));
                        raw.is_override = true;
                    }
                    if !cancelled && raw.start != 0 {
                        if raw.end <= raw.start {
                            raw.end = raw.start + if raw.all_day { MIN_DAY } else { 0 };
                        }
                        feed.raw.push(raw);
                    }
                }
                continue;
            }
            let Some((raw, cancelled, rid)) = cur.as_mut() else { continue };
            let Some((head, value)) = line.split_once(':') else { continue };
            match head.split(';').next().unwrap_or("") {
                "UID" => raw.uid = value.to_string(),
                "SUMMARY" => raw.title = unescape(value),
                "STATUS" => *cancelled = value.eq_ignore_ascii_case("CANCELLED"),
                "DTSTART" => (raw.start, raw.all_day) = parse_dt(value, utc_offset).unwrap_or((0, false)),
                "DTEND" => raw.end = parse_dt(value, utc_offset).map_or(0, |d| d.0),
                "RRULE" => raw.rule = parse_rule(value, utc_offset),
                "RECURRENCE-ID" => *rid = parse_dt(value, utc_offset).map(|d| d.0),
                "EXDATE" => raw.exdates.extend(value.split(',').filter_map(|v| parse_dt(v, utc_offset)).map(|d| d.0)),
                _ => {}
            }
        }
        feed
    }

    pub fn merge(&mut self, other: Feed) {
        self.raw.extend(other.raw);
        self.overridden.extend(other.overridden);
    }

    // Eventos que tocan [from, to), ordenados por inicio.
    pub fn occurrences(&self, from: i64, to: i64) -> Vec<Event> {
        let mut out = Vec::new();
        for r in &self.raw {
            let dur = r.end - r.start;
            let starts = match &r.rule {
                Some(rule) => rule_starts(r.start, rule, from, to),
                None => vec![r.start],
            };
            for s in starts {
                let end = s + dur;
                if s < to && end.max(s + 1) > from && !r.exdates.contains(&s) && (r.is_override || !self.overridden.contains(&(r.uid.clone(), s))) {
                    out.push(Event { start: s, end, all_day: r.all_day, title: r.title.clone() });
                }
            }
        }
        out.sort_by_key(|e| (e.start, e.title.clone()));
        out
    }
}

fn rule_starts(start: i64, rule: &Rule, from: i64, to: i64) -> Vec<i64> {
    let (base, tod) = (start.div_euclid(MIN_DAY), start.rem_euclid(MIN_DAY));
    let (y0, m0, d0) = civil_from_days(base);
    let (mut out, mut n) = (Vec::new(), 0u32);
    // Sin COUNT no hace falta contar desde el inicio: se arranca una vuelta antes de `from`.
    let unit = rule.interval * if rule.freq == Freq::Weekly { 7 } else { 1 };
    let k0 = if rule.count.is_none() && matches!(rule.freq, Freq::Daily | Freq::Weekly) { ((from.div_euclid(MIN_DAY) - base) / unit - 1).max(0) } else { 0 };
    for k in k0..k0 + MAX_STEPS {
        let step = k * rule.interval;
        let mut days: Vec<i64> = match rule.freq {
            Freq::Daily => vec![base + step],
            Freq::Weekly if rule.byday.is_empty() => vec![base + 7 * step],
            Freq::Weekly => rule.byday.iter().map(|&(_, wd)| base - weekday(base) as i64 + 7 * step + wd as i64).filter(|d| *d >= base).collect(),
            Freq::Monthly | Freq::Yearly => {
                let t = y0 * 12 + (m0 as i64 - 1) + step * if rule.freq == Freq::Yearly { 12 } else { 1 };
                let (y, m) = (t.div_euclid(12), (t.rem_euclid(12) + 1) as u32);
                if rule.freq == Freq::Monthly && !rule.byday.is_empty() {
                    rule.byday
                        .iter()
                        .flat_map(|&(ord, wd)| match ord {
                            0 => (1..=days_in_month(y, m)).filter(|d| weekday(days_from_civil(y, m, *d)) == wd).collect(),
                            _ => nth_weekday(y, m, ord, wd).into_iter().collect::<Vec<_>>(),
                        })
                        .map(|d| days_from_civil(y, m, d))
                        .collect()
                } else if d0 <= days_in_month(y, m) {
                    vec![days_from_civil(y, m, d0)]
                } else {
                    vec![]
                }
            }
        };
        days.retain(|d| *d >= base);
        days.sort_unstable();
        days.dedup();
        for d in days {
            let s = d * MIN_DAY + tod;
            if s >= to || rule.until.is_some_and(|u| s > u) {
                return out;
            }
            n += 1;
            if rule.count.is_some_and(|c| n > c) {
                return out;
            }
            out.push(s);
        }
    }
    out
}

// La URL secreta va por stdin (`-K -`): en la línea de comandos la vería cualquier proceso local.
fn download(url: &str) -> Option<String> {
    use std::io::Write;
    use std::process::Stdio;
    if url.contains(['\r', '\n']) {
        return None;
    }
    let mut child = std::process::Command::new("curl.exe")
        .args(["-sL", "--fail", "--max-time", "20", "--proto", "=https", "--proto-redir", "=https", "--max-filesize", "20000000", "-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .ok()?;
    let line = format!("url = \"{}\"\n", url.replace('\\', "\\\\").replace('"', "\\\""));
    child.stdin.take()?.write_all(line.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

// Baja los feeds de `user.json` ("calendar": [urls]) con curl.exe cada 15 min; cada minuto mientras no haya URL o falle.
pub fn spawn_feed(theme_path: std::path::PathBuf) {
    std::thread::spawn(move || loop {
        let urls = crate::theme::Theme::load(&theme_path).map(|t| t.calendar).unwrap_or_default();
        let mut wait = 60;
        if !urls.is_empty() {
            let offset = crate::calendar::utc_offset_min();
            let mut merged = Feed::default();
            let mut ok = true;
            for url in &urls {
                match download(url) {
                    Some(body) if body.contains("BEGIN:VCALENDAR") => merged.merge(Feed::parse(&body, offset)),
                    _ => ok = false,
                }
            }
            // Un feed caído o una página de login no deben borrar lo que ya se tenía.
            if ok {
                *FEED.lock().unwrap() = merged;
                wait = 900;
            }
        } else {
            *FEED.lock().unwrap() = Feed::default();
        }
        std::thread::sleep(Duration::from_secs(wait));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i64, m: u32, d: u32) -> i64 {
        days_from_civil(y, m, d) * MIN_DAY
    }

    fn feed(body: &str) -> Feed {
        Feed::parse(&format!("BEGIN:VCALENDAR\r\n{body}\r\nEND:VCALENDAR\r\n"), -180)
    }

    fn vevent(props: &str) -> String {
        format!("BEGIN:VEVENT\r\n{props}\r\nEND:VEVENT")
    }

    #[test]
    fn utc_events_move_to_local_time_and_titles_are_unescaped_and_unfolded() {
        let f = feed(&vevent("UID:a\r\nDTSTART:20261108T130000Z\r\nDTEND:20261108T140000Z\r\nSUMMARY:Reunión\\, con\r\n  Ana"));
        let e = f.occurrences(day(2026, 11, 8), day(2026, 11, 9));
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].start, e[0].end), (day(2026, 11, 8) + 10 * 60, day(2026, 11, 8) + 11 * 60));
        assert_eq!(e[0].title, "Reunión, con Ana");
    }

    #[test]
    fn all_day_event_spans_its_days_and_dtend_is_exclusive() {
        let f = feed(&vevent("UID:b\r\nDTSTART;VALUE=DATE:20261110\r\nDTEND;VALUE=DATE:20261112\r\nSUMMARY:Viaje"));
        assert!(f.occurrences(day(2026, 11, 10), day(2026, 11, 11))[0].all_day);
        assert_eq!(f.occurrences(day(2026, 11, 11), day(2026, 11, 12)).len(), 1);
        assert!(f.occurrences(day(2026, 11, 12), day(2026, 11, 13)).is_empty());
    }

    #[test]
    fn weekly_rule_with_byday_count_and_exdate() {
        let f = feed(&vevent("UID:c\r\nDTSTART:20261102T120000\r\nDTEND:20261102T130000\r\nSUMMARY:Daily\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=5\r\nEXDATE:20261104T120000"));
        let starts: Vec<i64> = f.occurrences(day(2026, 11, 1), day(2026, 12, 31)).iter().map(|e| e.start).collect();
        // lun 2, (mié 4 excluido), lun 9, mié 11, lun 16: COUNT=5 cuenta también la excluida
        assert_eq!(starts, [day(2026, 11, 2) + 720, day(2026, 11, 9) + 720, day(2026, 11, 11) + 720, day(2026, 11, 16) + 720]);
    }

    #[test]
    fn moved_instance_replaces_the_original() {
        let f = feed(&format!(
            "{}\r\n{}",
            vevent("UID:d\r\nDTSTART:20261102T120000\r\nSUMMARY:Sync\r\nRRULE:FREQ=DAILY;COUNT=3"),
            vevent("UID:d\r\nRECURRENCE-ID:20261103T120000\r\nDTSTART:20261103T150000\r\nSUMMARY:Sync movido")
        ));
        let e = f.occurrences(day(2026, 11, 3), day(2026, 11, 4));
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].title.as_str(), e[0].start), ("Sync movido", day(2026, 11, 3) + 900));
    }

    #[test]
    fn monthly_nth_weekday_and_yearly() {
        let f = feed(&vevent("UID:e\r\nDTSTART:20261110T090000\r\nSUMMARY:Mensual\r\nRRULE:FREQ=MONTHLY;BYDAY=2TU"));
        let e = f.occurrences(day(2026, 11, 1), day(2027, 2, 1));
        assert_eq!(e.iter().map(|e| e.start).collect::<Vec<_>>(), [day(2026, 11, 10) + 540, day(2026, 12, 8) + 540, day(2027, 1, 12) + 540]);
        let y = feed(&vevent("UID:f\r\nDTSTART;VALUE=DATE:20200229\r\nSUMMARY:Bisiesto\r\nRRULE:FREQ=YEARLY"));
        assert_eq!(y.occurrences(day(2024, 1, 1), day(2025, 1, 1)).len(), 1);
        assert!(y.occurrences(day(2026, 1, 1), day(2027, 1, 1)).is_empty());
    }

    #[test]
    fn until_stops_the_series_and_cancelled_events_are_dropped() {
        let f = feed(&format!(
            "{}\r\n{}",
            vevent("UID:g\r\nDTSTART:20261101T100000\r\nSUMMARY:Corta\r\nRRULE:FREQ=DAILY;UNTIL=20261103T235959Z"),
            vevent("UID:h\r\nDTSTART:20261105T100000\r\nSUMMARY:Cancelada\r\nSTATUS:CANCELLED")
        ));
        let e = f.occurrences(day(2026, 11, 1), day(2026, 12, 1));
        assert_eq!(e.len(), 3);
        assert!(e.iter().all(|e| e.title == "Corta"));
    }

    #[test]
    fn edited_instance_at_the_same_time_is_kept_once() {
        let f = feed(&format!(
            "{}\r\n{}",
            vevent("UID:j\r\nDTSTART:20261102T120000\r\nSUMMARY:Sync\r\nRRULE:FREQ=DAILY;COUNT=3"),
            vevent("UID:j\r\nRECURRENCE-ID:20261103T120000\r\nDTSTART:20261103T120000\r\nSUMMARY:Sync editado")
        ));
        let e = f.occurrences(day(2026, 11, 3), day(2026, 11, 4));
        assert_eq!(e.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["Sync editado"]);
    }

    #[test]
    fn hostile_or_unsupported_rules_do_not_panic_or_misexpand() {
        let f = feed(&vevent("UID:k\r\nDTSTART:20261102T120000\r\nSUMMARY:x\r\nRRULE:FREQ=WEEKLY;BYDAY=éa"));
        assert!(f.occurrences(day(2026, 11, 1), day(2027, 1, 1)).len() > 1, "sin BYDAY válido sigue siendo semanal desde DTSTART");
        let y = feed(&vevent("UID:l\r\nDTSTART:20200510T090000\r\nSUMMARY:Dia de la madre\r\nRRULE:FREQ=YEARLY;BYMONTH=5;BYDAY=2SU"));
        assert_eq!(y.occurrences(day(2020, 1, 1), day(2027, 1, 1)).len(), 1, "regla no soportada: solo la primera ocurrencia, no una fecha inventada");
    }

    #[test]
    fn old_daily_series_still_show_today() {
        let f = feed(&vevent("UID:m\r\nDTSTART:20100101T080000\r\nSUMMARY:Vieja\r\nRRULE:FREQ=DAILY"));
        assert_eq!(f.occurrences(day(2026, 11, 8), day(2026, 11, 9)).len(), 1);
    }

    #[test]
    fn garbage_input_yields_an_empty_feed() {
        assert!(Feed::parse("<html>403</html>", 0).is_empty());
        assert!(feed(&vevent("UID:i\r\nSUMMARY:sin fecha")).is_empty());
    }
}
