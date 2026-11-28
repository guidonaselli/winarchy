// Árbol UIA de la isla. Las acciones repiten el clic en el centro del control, así hay una sola ruta de comportamiento.
use accesskit::{Action, Node, Role};
use crate::i18n::{t as tr, K};
use crate::layout::{Layout, R};
use crate::{calendar, wpn, Panel};

pub struct Bar<'a> {
    pub clock: &'a str,
    pub temp: &'a str,
    pub island: &'a str,
    pub pinned: bool,
}

fn node(layout: &Layout, role: Role, label: String, r: R) -> Node {
    let r = layout.place(r);
    let mut node = Node::new(role);
    // Un texto fijo toma su nombre del valor, no de la etiqueta.
    if role == Role::Label { node.set_value(label) } else { node.set_label(label) }
    node.set_bounds(accesskit::Rect::new(r.x as f64, r.y as f64, (r.x + r.w) as f64, (r.y + r.h) as f64));
    node
}

pub fn nodes(layout: &Layout, bar: &Bar, panel: Option<&Panel>) -> Vec<(String, Node)> {
    let mut out: Vec<(String, Node)> = Vec::new();
    let mut add = |key: String, role: Role, label: String, r: R, selected: bool| {
        if r.w <= 0 || r.h <= 0 || label.is_empty() {
            return;
        }
        let mut n = node(layout, role, label, r);
        if role == Role::Button {
            n.add_action(Action::Click);
        }
        if selected {
            n.set_selected(true);
        }
        out.push((key, n));
    };
    let workspace = |ws: usize| format!("{} {}", tr(K::Workspace), ws + 1);
    for d in &layout.dots {
        add(format!("dot{}", d.ws), Role::Button, workspace(d.ws), R { y: 0, h: layout.bar_h, ..d.r }, d.focused);
    }
    add("temp".into(), Role::Button, bar.temp.into(), layout.temp, false);
    add("island".into(), Role::Label, bar.island.into(), layout.label, false);
    add("pin".into(), Role::Button, tr(K::PinPanel).into(), layout.pin, bar.pinned);
    add("clock".into(), Role::Button, bar.clock.into(), layout.clock, false);

    let (Some(p), Some(pr)) = (panel, &layout.panel) else { return out };
    if let Some(m) = &p.media {
        add("track".into(), Role::Label, format!("{} - {}", m.title, m.artist), pr.title, false);
        add("prev".into(), Role::Button, tr(K::MediaPrevious).into(), pr.prev, false);
        add("toggle".into(), Role::Button, tr(K::MediaPlayPause).into(), pr.toggle, false);
        add("next".into(), Role::Button, tr(K::MediaNext).into(), pr.next, false);
    }
    if let Some(w) = &p.weather {
        let [d1, d2] = w.detail(crate::lang());
        add("weather".into(), Role::Button, format!("{} {}. {d1}. {d2}", w.title(crate::lang()), w.range()), pr.weather, false);
    }
    for (i, (m, (name, _, value))) in pr.meters.iter().zip(&p.meters).enumerate() {
        let row = R { w: m.value.x + m.value.w - m.label.x, ..m.label };
        if !matches!(name, crate::Meter::Vol | crate::Meter::Bri) {
            add(format!("meter{i}"), Role::Label, format!("{} {value}", name.name()), row, false);
        }
    }
    if let Some(cv) = &p.cal {
        add("title".into(), Role::Label, cv.title.clone(), pr.cal_title, false);
        add("calprev".into(), Role::Button, tr(K::PreviousMonth).into(), pr.cal_prev, false);
        add("calnext".into(), Role::Button, tr(K::NextMonth).into(), pr.cal_next, false);
        for (i, (r, day)) in pr.cal_cells.iter().zip(&cv.cells).enumerate() {
            let Some(day) = *day else { continue };
            let mark = if cv.marks[day as usize] { " •" } else { "" };
            add(format!("day{i}"), Role::Button, format!("{day} {}{mark}", cv.title), *r, cv.sel == Some(day));
        }
        for (i, (line, r)) in cv.agenda.iter().zip(&pr.agenda).enumerate() {
            add(format!("agenda{i}"), Role::Label, line.clone(), *r, false);
        }
        add("world".into(), Role::Label, cv.world.clone(), pr.world, false);
        if let Some((head, rows)) = pr.notifs.split_first() {
            add("notifs".into(), Role::Button, tr(K::RecentNotifications).into(), *head, false);
            for (i, (n, r)) in cv.notifs.iter().zip(rows).enumerate() {
                let (hh, mm) = wpn::arrival_clock(n.arrival, calendar::utc_offset_min());
                let app = n.app.as_deref().map(|a| format!("{}: ", wpn::app_label(a))).unwrap_or_default();
                add(format!("notif{i}"), Role::Button, format!("{hh:02}:{mm:02} {app}{} {}", n.title, n.text), *r, false);
            }
        }
    }
    for c in &pr.chips {
        add(format!("chip{}", c.ws), Role::Button, workspace(c.ws), c.r, c.focused);
    }
    let mode = crate::komorebi::CURRENT.lock().unwrap().as_ref().map(|w| w.mode).unwrap_or_default();
    add("mode".into(), Role::Button, crate::mode_label(mode).into(), pr.mode, false);
    add("notes".into(), Role::Label, p.notes.clone(), pr.notes, false);
    for (m, (name, pct, value)) in pr.meters.iter().zip(&p.meters) {
        let key = match name {
            crate::Meter::Vol => "vol",
            crate::Meter::Bri => "bri",
            _ => continue,
        };
        let mut slider = node(layout, Role::Slider, format!("{} {value}", name.name()), R { w: m.value.x + m.value.w - m.label.x, ..m.label });
        slider.set_numeric_value(f64::from(*pct));
        slider.set_min_numeric_value(0.0);
        slider.set_max_numeric_value(100.0);
        slider.add_action(Action::Increment);
        slider.add_action(Action::Decrement);
        out.push((key.into(), slider));
    }
    out
}

// Punto de cliente donde un clic equivale a activar el control `key`.
pub fn click_point(nodes: &[(String, Node)], key: &str) -> Option<(i32, i32)> {
    let node = &nodes.iter().find(|(k, n)| k == key && n.supports_action(Action::Click))?.1;
    let r = node.bounds()?;
    Some((((r.x0 + r.x1) / 2.0) as i32, ((r.y0 + r.y1) / 2.0) as i32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{bar_h, panel_height, Hit, Inputs, PanelShape};

    #[test]
    fn activating_a_node_clicks_its_own_control() {
        for flip in [false, true] {
            let shape = PanelShape { cal_rows: 5, agenda: 3, world: true, notifs: 2, ..Default::default() };
            let dots = [(0, true), (1, false)];
            let layout = Layout::new(&Inputs { scale: 1.0, width: 360, height: bar_h(1.0) + panel_height(1.0, shape), flip, clock_w: 40, orb: false, temp_w: 30, dots: &dots, panel: Some(shape) });
            let toast = wpn::Toast { arrival: 0, title: "t".into(), text: "b".into(), app: None };
            let cal = crate::CalView { title: "mayo 2026".into(), rows: 5, cells: (0..35).map(|i| (i < 31).then(|| i + 1)).collect(), today: None, sel: Some(2), marks: vec![false; 32], agenda: vec!["a".into(); 3], world: "w".into(), notifs: vec![toast.clone(), toast], notif_pages: 1 };
            let panel = Panel { media: None, weather: None, meters: Vec::new(), notes: "n".into(), cal: Some(cal) };
            let bar = Bar { clock: "10:00", temp: "20°", island: "", pinned: false };
            let nodes = nodes(&layout, &bar, Some(&panel));
            let hit = |key: &str| click_point(&nodes, key).and_then(|(x, y)| layout.hit(x, y));
            assert_eq!(hit("clock"), Some(Hit::Clock));
            assert_eq!(hit("temp"), Some(Hit::Temp));
            assert_eq!(hit("pin"), Some(Hit::Pin));
            assert_eq!(hit("dot1"), Some(Hit::Workspace(1)));
            assert_eq!(hit("calprev"), Some(Hit::CalPrev));
            assert_eq!(hit("calnext"), Some(Hit::CalNext));
            assert_eq!(hit("day7"), Some(Hit::CalCell(7)));
            assert_eq!(hit("notifs"), Some(Hit::NotifPage));
            assert_eq!(hit("notif1"), Some(Hit::Notif(1)));
            assert_eq!(click_point(&nodes, "title"), None);
            assert!(nodes.iter().any(|(k, n)| k == "title" && n.value() == Some("mayo 2026")));
            assert!(nodes.iter().all(|(k, _)| k != "day31"));
        }
    }
}
