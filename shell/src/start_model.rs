// Modelo del menú Inicio propio: lista de apps, pineados, búsqueda y geometría. Sin Win32: lo que se lee del sistema entra como datos.
use crate::layout::{px, R};

#[derive(Clone, Debug, PartialEq)]
pub struct App {
    pub name: String,
    pub open: String,
}

pub const PIN_COLS: usize = 6;
pub const PIN_ROWS: usize = 3;
pub const LIST_ROWS: usize = 7;
pub const PLACE_COLS: usize = 4;
pub const PLACE_ROWS: usize = 2;

// Nombres (en inglés o español) de los pineados por defecto, en orden; el primer app que contiene el texto gana.
const DEFAULT_PINS: &[&[&str]] = &[
    &["edge"], &["files", "explorador"], &["wezterm"], &["settings", "configuración"], &["calculator", "calculadora"], &["notepad", "bloc de notas"],
    &["store"], &["photos", "fotos"], &["paint"], &["terminal"], &["flow launcher"], &["task manager", "administrador de tareas"],
];

pub fn sort_dedupe(apps: &mut Vec<App>) {
    apps.sort_by_cached_key(|a| (a.name.to_lowercase(), a.open.clone()));
    apps.dedup_by(|a, b| a.open.eq_ignore_ascii_case(&b.open));
}

pub fn default_pins(apps: &[App]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for names in DEFAULT_PINS {
        let free = |a: &&App| !out.contains(&a.open);
        // exacto, luego prefijo, luego contiene: "Settings" no debe caer en "Reset settings"
        let tiers: [&dyn Fn(&str, &str) -> bool; 3] = [&|n, w| n == w, &|n, w| n.starts_with(w), &|n, w| n.contains(w)];
        let hit = tiers.iter().find_map(|tier| apps.iter().filter(free).find(|a| {
            let n = a.name.to_lowercase();
            names.iter().any(|w| tier(&n, w)) && !n.contains("uninstall") && !n.contains("reset")
        }));
        if let Some(a) = hit {
            out.push(a.open.clone());
        }
    }
    out
}

// Índices de `apps` que cumplen la consulta: primero los que empiezan por ella o por una palabra, después los que la contienen.
pub fn filter(apps: &[App], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return (0..apps.len()).collect();
    }
    let rank = |name: &str| -> Option<u8> {
        let n = name.to_lowercase();
        if n.starts_with(&q) {
            Some(0)
        } else if n.split([' ', '-', '_', '.']).any(|w| w.starts_with(&q)) {
            Some(1)
        } else {
            n.contains(&q).then_some(2)
        }
    };
    let mut hits: Vec<(u8, usize)> = apps.iter().enumerate().filter_map(|(i, a)| rank(&a.name).map(|r| (r, i))).collect();
    hits.sort_by_key(|(r, i)| (*r, *i));
    hits.into_iter().map(|(_, i)| i).collect()
}

pub fn is_app(open: &str) -> bool {
    open.to_ascii_lowercase().starts_with(r"shell:appsfolder\")
}

// Nombre visible de un pin que no es app, solo con el texto de la ruta (sin tocar el disco).
pub fn place_name(open: &str) -> String {
    let t = open.trim_end_matches(['\\', '/']);
    let leaf = t.rsplit(['\\', '/']).next().unwrap_or(t);
    let stem = match leaf.rsplit_once('.') {
        Some((s, ext)) if !s.is_empty() && matches!(ext.to_ascii_lowercase().as_str(), "lnk" | "url") => s,
        _ => leaf,
    };
    if stem.is_empty() { open.to_string() } else { stem.to_string() }
}

// Apps seguidas de los lugares anclados (sin repetir), para que búsqueda y pineados compartan índices.
pub fn catalog(apps: &[App], saved: &[String]) -> Vec<App> {
    let mut out = apps.to_vec();
    for open in saved.iter().filter(|o| !is_app(o)) {
        if !out.iter().any(|a| a.open.eq_ignore_ascii_case(open)) {
            out.push(App { name: place_name(open), open: open.clone() });
        }
    }
    out
}

// Índices de `cat` para la grilla de apps y para la franja de lugares, cada uno con su propio tope.
// Los pineados guardados que ya no existen se descartan sin error: una app desinstalada no rompe el menú.
pub fn pinned(cat: &[App], saved: &[String]) -> (Vec<usize>, Vec<usize>) {
    let (mut grid, mut places) = (Vec::new(), Vec::new());
    for open in saved {
        let Some(i) = cat.iter().position(|a| a.open.eq_ignore_ascii_case(open)) else { continue };
        let list = if is_app(open) { &mut grid } else { &mut places };
        if !list.contains(&i) {
            list.push(i);
        }
    }
    grid.truncate(PIN_COLS * PIN_ROWS);
    places.truncate(PLACE_COLS * PLACE_ROWS);
    (grid, places)
}

pub enum Saved {
    Absent,
    Bad,
    Ok(Vec<String>),
}

pub fn read_pins(path: &std::path::Path) -> Saved {
    match std::fs::read_to_string(path) {
        Err(_) => Saved::Absent,
        Ok(text) => serde_json::from_str(&text).map_or(Saved::Bad, Saved::Ok),
    }
}

// Lista sobre la que se puede escribir sin perder pines: nunca sobre un JSON roto ni sembrando defaults antes de conocer las apps.
pub fn base_for_write(saved: Saved, apps: &[App]) -> Option<Vec<String>> {
    match saved {
        Saved::Ok(v) => Some(v),
        Saved::Absent if !apps.is_empty() => Some(default_pins(apps)),
        _ => None,
    }
}

fn same_place(a: &str, b: &str) -> bool {
    a.trim_end_matches(['\\', '/']).eq_ignore_ascii_case(b.trim_end_matches(['\\', '/']))
}

pub fn pin(mut base: Vec<String>, open: &str) -> Vec<String> {
    let open = open.strip_prefix(r"\\?\").unwrap_or(open);
    if !base.iter().any(|b| same_place(b, open)) {
        base.push(open.to_string());
    }
    base
}

pub fn unpin(mut base: Vec<String>, open: &str) -> Vec<String> {
    base.retain(|b| !same_place(b, open));
    base
}

pub fn save_pins(path: &std::path::Path, pins: &[String]) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(pins).map_err(std::io::Error::other)?)?;
    std::fs::rename(&tmp, path)
}

// Mueve la selección en la grilla de pineados o en la lista; `cols` es 1 en la lista.
pub fn step(current: Option<usize>, len: usize, dx: i32, dy: i32, cols: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let Some(cur) = current else { return Some(0) };
    let next = cur as i32 + dx + dy * cols as i32;
    Some(next.clamp(0, len as i32 - 1) as usize)
}

// Selección sobre la grilla de apps seguida de la franja de lugares: bajar de la última fila de la grilla entra a la franja y subir de su primera fila vuelve.
pub fn step_sections(current: Option<usize>, grid_len: usize, places_len: usize, dx: i32, dy: i32) -> Option<usize> {
    let Some(cur) = current else { return (grid_len + places_len > 0).then_some(0) };
    if cur < grid_len {
        let last_row = (grid_len - 1) / PIN_COLS;
        if dy > 0 && cur / PIN_COLS == last_row && places_len > 0 {
            return Some(grid_len + ((cur % PIN_COLS) * PLACE_COLS / PIN_COLS).min(places_len - 1));
        }
        return step(current, grid_len, dx, dy, PIN_COLS);
    }
    let k = cur - grid_len;
    if dy < 0 && k < PLACE_COLS && grid_len > 0 {
        return Some(((grid_len - 1) / PIN_COLS * PIN_COLS + k * PIN_COLS / PLACE_COLS).min(grid_len - 1));
    }
    step(Some(k), places_len, dx, dy, PLACE_COLS).map(|i| grid_len + i)
}

// Desplazamiento (en filas) mínimo para que la fila `index` quede dentro de la ventana de `LIST_ROWS`.
pub fn scroll_to(offset: usize, index: usize) -> usize {
    if index < offset {
        index
    } else if index >= offset + LIST_ROWS {
        index + 1 - LIST_ROWS
    } else {
        offset
    }
}

pub struct Layout {
    pub size: (i32, i32),
    pub search: R,
    pub header: R,
    pub all_button: R,
    pub cells: Vec<R>,
    pub rows: Vec<R>,
    pub footer: R,
    pub power: R,
    pub divider: R,
    pub places: Vec<R>,
}

const WIDTH_DIP: i32 = 600;
const PAD_DIP: i32 = 20;
const SEARCH_DIP: i32 = 40;
const HEADER_DIP: i32 = 32;
const CELL_H_DIP: i32 = 84;
const ROW_DIP: i32 = 40;
const FOOTER_DIP: i32 = 56;
const PLACE_H_DIP: i32 = 36;

pub fn layout(scale: f32) -> Layout {
    let (pad, w) = (px(PAD_DIP, scale), px(WIDTH_DIP, scale));
    let inner = w - 2 * pad;
    let search = R { x: pad, y: pad, w: inner, h: px(SEARCH_DIP, scale) };
    let header = R { x: pad, y: search.y + search.h + px(12, scale), w: inner, h: px(HEADER_DIP, scale) };
    let all_button = R { x: header.x + header.w - px(170, scale), y: header.y, w: px(170, scale), h: header.h };
    let top = header.y + header.h;
    let (cw, ch) = (inner / PIN_COLS as i32, px(CELL_H_DIP, scale));
    let cells = (0..PIN_COLS * PIN_ROWS).map(|i| R { x: pad + (i % PIN_COLS) as i32 * cw, y: top + (i / PIN_COLS) as i32 * ch, w: cw, h: ch }).collect();
    let rh = px(ROW_DIP, scale);
    let rows = (0..LIST_ROWS).map(|i| R { x: pad, y: top + i as i32 * rh, w: inner, h: rh }).collect();
    let (ph, pt) = (px(PLACE_H_DIP, scale), top + PIN_ROWS as i32 * ch + px(8, scale));
    let pw = inner / PLACE_COLS as i32;
    let places = (0..PLACE_COLS * PLACE_ROWS).map(|i| R { x: pad + (i % PLACE_COLS) as i32 * pw, y: pt + (i / PLACE_COLS) as i32 * ph, w: pw, h: ph }).collect();
    let divider = R { x: pad, y: pt - px(4, scale), w: inner, h: px(1, scale) };
    let body_bottom = (pt + PLACE_ROWS as i32 * ph).max(top + LIST_ROWS as i32 * rh);
    let footer = R { x: 0, y: body_bottom + px(12, scale), w, h: px(FOOTER_DIP, scale) };
    let power = R { x: w - pad - px(40, scale), y: footer.y + (footer.h - px(40, scale)) / 2, w: px(40, scale), h: px(40, scale) };
    Layout { size: (w, footer.y + footer.h), search, header, all_button, cells, rows, footer, power, divider, places }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, open: &str) -> App {
        App { name: name.into(), open: open.into() }
    }

    fn sample() -> Vec<App> {
        vec![app("Calculator", "c"), app("Microsoft Edge", "e"), app("Notepad", "n"), app("WezTerm", "w"), app("Paint", "p"), app("Uninstall Edge", "u")]
    }

    #[test]
    fn apps_sort_case_insensitively_and_drop_duplicate_targets() {
        let mut apps = vec![app("zeta", "z"), app("Alpha", "a"), app("alpha again", "A")];
        sort_dedupe(&mut apps);
        assert_eq!(apps.iter().map(|a| a.open.as_str()).collect::<Vec<_>>(), ["a", "z"]);
    }

    #[test]
    fn default_pins_follow_the_known_order_and_skip_uninstallers() {
        assert_eq!(default_pins(&sample()), ["e", "w", "c", "n", "p"]);
        let apps = vec![app("Reset settings", "r"), app("Settings", "s"), app("Microsoft Edge", "e")];
        assert_eq!(default_pins(&apps), ["e", "s"]);
    }

    #[test]
    fn filter_ranks_prefix_then_word_then_substring() {
        let apps = vec![app("Visual Notepad", "1"), app("Notepad", "2"), app("Keynote pad", "3")];
        assert_eq!(filter(&apps, "note"), [1, 0, 2]);
        assert_eq!(filter(&apps, "  "), [0, 1, 2]);
        assert!(filter(&apps, "zzz").is_empty());
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    const APP: &str = r"shell:AppsFolder\x";

    #[test]
    fn app_pins_are_told_apart_from_places_ignoring_case() {
        assert!(is_app(r"shell:AppsFolder\Microsoft.Edge"));
        assert!(is_app(r"SHELL:appsfolder\x"));
        assert!(!is_app(r"C:\Users\me\Projects"));
        assert!(!is_app("https://example.com"));
    }

    #[test]
    fn place_names_come_from_the_path_without_io() {
        assert_eq!(place_name(r"C:\"), "C:");
        assert_eq!(place_name(r"\\nas\share\"), "share");
        assert_eq!(place_name(r"C:\a\b\"), "b");
        assert_eq!(place_name(r"C:\a\Repo.lnk"), "Repo");
        assert_eq!(place_name(r"C:\a\x.url"), "x");
        assert_eq!(place_name(r"C:\a\notes.txt"), "notes.txt");
    }

    #[test]
    fn saved_pins_ignore_missing_apps_and_cap_at_the_grid() {
        let apps = sample();
        let cat = catalog(&apps, &[]);
        assert!(pinned(&cat, &strings(&["w", "gone", "C"])).0.is_empty(), "ids that are not AppsFolder entries never land in the app grid");
        let many: Vec<App> = (0..30).map(|i| app(&format!("a{i}"), &format!(r"shell:AppsFolder\o{i}"))).collect();
        let all: Vec<String> = many.iter().map(|a| a.open.clone()).collect();
        assert_eq!(pinned(&catalog(&many, &all), &all).0.len(), PIN_COLS * PIN_ROWS);
    }

    #[test]
    fn pins_partition_into_grid_and_places_each_with_its_own_cap() {
        let apps: Vec<App> = (0..20).map(|i| app(&format!("a{i}"), &format!(r"shell:AppsFolder\o{i}"))).collect();
        let mut saved: Vec<String> = apps.iter().map(|a| a.open.clone()).collect();
        saved.extend((0..10).map(|i| format!(r"C:\dir{i}")));
        let cat = catalog(&apps, &saved);
        let (grid, places) = pinned(&cat, &saved);
        assert_eq!((grid.len(), places.len()), (PIN_COLS * PIN_ROWS, PLACE_COLS * PLACE_ROWS));
        assert_eq!(cat[places[0]].open, r"C:\dir0");
        assert_eq!(cat[places[0]].name, "dir0");
    }

    #[test]
    fn places_survive_when_apps_are_not_loaded_yet() {
        let saved = strings(&[APP, r"D:\Work"]);
        let cat = catalog(&[], &saved);
        assert_eq!(pinned(&cat, &saved), (vec![], vec![0]));
    }

    #[test]
    fn pinning_edits_the_raw_list_keeps_unresolved_apps_and_dedupes() {
        let base = strings(&[APP, r"C:\Work"]);
        assert_eq!(pin(base.clone(), r"c:\work\"), base);
        assert_eq!(pin(base.clone(), r"\\?\D:\New"), strings(&[APP, r"C:\Work", r"D:\New"]));
        assert_eq!(unpin(base, r"C:\WORK"), strings(&[APP]));
    }

    #[test]
    fn writing_never_seeds_before_apps_load_nor_overwrites_bad_json() {
        assert_eq!(base_for_write(Saved::Absent, &[]), None);
        assert_eq!(base_for_write(Saved::Bad, &sample()), None);
        assert_eq!(base_for_write(Saved::Ok(strings(&["k"])), &[]), Some(strings(&["k"])));
        assert!(base_for_write(Saved::Absent, &sample()).is_some());
    }

    #[test]
    fn read_and_save_round_trip_and_flag_bad_files() {
        let dir = std::env::temp_dir().join(format!("winarchy-pins-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.json");
        assert!(matches!(read_pins(&path), Saved::Absent));
        save_pins(&path, &strings(&["a", r"C:\x"])).unwrap();
        assert!(matches!(read_pins(&path), Saved::Ok(v) if v == strings(&["a", r"C:\x"])));
        std::fs::write(&path, "{ not json").unwrap();
        assert!(matches!(read_pins(&path), Saved::Bad));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_finds_places() {
        let cat = catalog(&[app("Edge", APP)], &strings(&[r"C:\Work\Projects"]));
        assert_eq!(filter(&cat, "proj"), [1]);
    }

    #[test]
    fn sections_cross_between_grid_and_strip() {
        assert_eq!(step_sections(None, 0, 0, 1, 0), None);
        assert_eq!(step_sections(None, 0, 3, 1, 0), Some(0));
        assert_eq!(step_sections(Some(10), 12, 3, 0, 1), Some(12 + 2), "column 4 of 6 on the last row maps to chip 2 of 4");
        assert_eq!(step_sections(Some(1), 8, 3, 0, 1), Some(7), "not the last row: moves down inside the grid, clamped");
        assert_eq!(step_sections(Some(8 + 1), 8, 3, 0, -1), Some(6 + 1), "up from the strip lands on the last grid row");
        assert_eq!(step_sections(Some(8), 8, 3, 1, 0), Some(9));
        assert_eq!(step_sections(Some(10), 8, 3, 1, 0), Some(10));
    }

    #[test]
    fn selection_moves_in_the_grid_and_stays_inside() {
        assert_eq!(step(None, 10, 1, 0, 6), Some(0));
        assert_eq!(step(Some(0), 10, 0, 1, 6), Some(6));
        assert_eq!(step(Some(8), 10, 0, 1, 6), Some(9));
        assert_eq!(step(Some(0), 10, -1, 0, 6), Some(0));
        assert_eq!(step(None, 0, 1, 0, 6), None);
    }

    #[test]
    fn scrolling_keeps_the_selected_row_visible() {
        assert_eq!(scroll_to(0, 3), 0);
        assert_eq!(scroll_to(0, LIST_ROWS), 1);
        assert_eq!(scroll_to(5, 2), 2);
    }

    #[test]
    fn layout_fits_every_cell_and_row_inside_the_window() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let l = layout(scale);
            let inside = |r: &R| r.x >= 0 && r.y >= 0 && r.x + r.w <= l.size.0 && r.y + r.h <= l.size.1;
            assert!(l.cells.iter().chain(&l.rows).chain(&l.places).chain([&l.search, &l.header, &l.all_button, &l.footer, &l.power]).all(inside), "scale {scale}");
        }
    }
}
