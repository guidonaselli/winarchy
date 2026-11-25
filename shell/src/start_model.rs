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

// Los pineados guardados que ya no existen se descartan sin error: una app desinstalada no rompe el menú.
pub fn pinned_indices(apps: &[App], saved: Option<Vec<String>>) -> Vec<usize> {
    let wanted = saved.unwrap_or_else(|| default_pins(apps));
    wanted.iter().filter_map(|open| apps.iter().position(|a| a.open.eq_ignore_ascii_case(open))).take(PIN_COLS * PIN_ROWS).collect()
}

pub fn load_pins(path: &std::path::Path) -> Option<Vec<String>> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
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
}

const WIDTH_DIP: i32 = 600;
const PAD_DIP: i32 = 20;
const SEARCH_DIP: i32 = 40;
const HEADER_DIP: i32 = 32;
const CELL_H_DIP: i32 = 84;
const ROW_DIP: i32 = 40;
const FOOTER_DIP: i32 = 56;

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
    let body_bottom = top + (PIN_ROWS as i32 * ch).max(LIST_ROWS as i32 * rh);
    let footer = R { x: 0, y: body_bottom + px(12, scale), w, h: px(FOOTER_DIP, scale) };
    let power = R { x: w - pad - px(40, scale), y: footer.y + (footer.h - px(40, scale)) / 2, w: px(40, scale), h: px(40, scale) };
    Layout { size: (w, footer.y + footer.h), search, header, all_button, cells, rows, footer, power }
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

    #[test]
    fn saved_pins_ignore_missing_apps_and_cap_at_the_grid() {
        let apps = sample();
        assert_eq!(pinned_indices(&apps, Some(vec!["w".into(), "gone".into(), "C".into()])), [3, 0]);
        let many: Vec<App> = (0..30).map(|i| app(&format!("a{i}"), &format!("o{i}"))).collect();
        let all: Vec<String> = many.iter().map(|a| a.open.clone()).collect();
        assert_eq!(pinned_indices(&many, Some(all)).len(), PIN_COLS * PIN_ROWS);
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
            assert!(l.cells.iter().chain(&l.rows).chain([&l.search, &l.header, &l.all_button, &l.footer, &l.power]).all(inside), "scale {scale}");
        }
    }
}
