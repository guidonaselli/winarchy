// Geometría de la isla en píxeles, sin Win32: la pinta `render_pill` y la consulta el hit-test del mouse.
// Todo se calcula con el borde pegado arriba; `place` lo espeja cuando la isla vive abajo.

const BAR_DIP: i32 = 24;
const PAD_DIP: i32 = 14;
const DOT_DIP: i32 = 6;
const DOT_ACTIVE_DIP: i32 = 18;
const GAP_DIP: i32 = 5;
const SEP_DIP: i32 = 12;
const CORNER_DIP: i32 = 14;
pub const PANEL_W_DIP: i32 = 300;
const MEDIA_DIP: i32 = 34;
const WEATHER_DIP: i32 = 46;
const BTN_DIP: i32 = 28;
const METER_DIP: i32 = 14;
const METER_GAP_DIP: i32 = 8;
const SECTION_GAP_DIP: i32 = 10;
const CHIP_W_DIP: i32 = 26;
const CHIP_H_DIP: i32 = 22;
const CHIP_GAP_DIP: i32 = 6;
const PANEL_TOP_DIP: i32 = 4;
const PANEL_BOTTOM_DIP: i32 = 14;
const PROGRESS_BAND_DIP: i32 = 6;
const CAL_HEAD_DIP: i32 = 22;
const CAL_WEEK_DIP: i32 = 14;
const CAL_CELL_DIP: i32 = 24;
const AGENDA_LINE_DIP: i32 = 17;
const ICON_DIP: i32 = 14;

pub fn px(dip: i32, scale: f32) -> i32 {
    (dip as f32 * scale).round() as i32
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct R {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl R {
    fn new(x: i32, y: i32, w: i32, h: i32) -> R {
        R { x, y, w: w.max(0), h }
    }

    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Workspace(usize),
    Prev,
    Toggle,
    Next,
    Pin,
    Weather,
    // Fila de medidor: índice y, si el clic cayó sobre la barra, su fracción 0..1.
    Meter(usize, Option<f32>),
    Clock,
    LayoutIcon,
    CalPrev,
    CalNext,
    // Celda de la grilla del mes, por índice.
    CalCell(usize),
    Surface,
}

#[derive(Clone, Default, Debug)]
pub struct Item {
    pub r: R,
    pub ws: usize,
    pub focused: bool,
}

#[derive(Clone, Default, Debug)]
pub struct Meter {
    pub label: R,
    pub track: R,
    pub value: R,
}

#[derive(Clone, Default, Debug)]
pub struct PanelRects {
    pub title: R,
    pub artist: R,
    pub weather: R,
    pub weather_title: R,
    pub weather_range: R,
    pub weather_detail: R,
    pub weather_detail2: R,
    pub prev: R,
    pub toggle: R,
    pub next: R,
    pub meters: Vec<Meter>,
    pub chips: Vec<Item>,
    pub notes: R,
    pub cal_title: R,
    pub cal_prev: R,
    pub cal_next: R,
    pub cal_heads: Vec<R>,
    pub cal_cells: Vec<R>,
    pub agenda: Vec<R>,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PanelShape {
    pub media: bool,
    pub weather: bool,
    pub meters: usize,
    // Panel del reloj: filas de la grilla del mes (0 = no es ese panel) y líneas de agenda.
    pub cal_rows: usize,
    pub agenda: usize,
}

pub struct Inputs<'a> {
    pub scale: f32,
    pub width: i32,
    pub height: i32,
    pub flip: bool,
    pub clock_w: i32,
    pub orb: bool,
    pub icon: bool,
    pub dots: &'a [(usize, bool)],
    pub panel: Option<PanelShape>,
}

#[derive(Clone, Default, Debug)]
pub struct Layout {
    pub width: i32,
    pub height: i32,
    pub flip: bool,
    pub bar_h: i32,
    pub orb: R,
    pub icon: R,
    pub dots: Vec<Item>,
    pub label: R,
    pub clock: R,
    pub progress: R,
    pub pin: R,
    pub panel: Option<PanelRects>,
}

pub fn bar_h(scale: f32) -> i32 {
    px(BAR_DIP, scale)
}

// Sitio que le quita a la etiqueta el botón de pin, que existe solo con el panel abierto.
pub fn pin_w(scale: f32) -> i32 {
    bar_h(scale) + px(SEP_DIP, scale)
}

pub fn progress_band(scale: f32) -> i32 {
    px(PROGRESS_BAND_DIP, scale)
}

fn dots_w(scale: f32, dots: &[(usize, bool)]) -> i32 {
    let w: i32 = dots.iter().map(|d| px(if d.1 { DOT_ACTIVE_DIP } else { DOT_DIP }, scale)).sum();
    w + px(GAP_DIP, scale) * (dots.len() as i32 - 1).max(0)
}

fn lead_w(scale: f32, orb: bool, icon: bool, dots: &[(usize, bool)]) -> i32 {
    let orb_w = if orb { bar_h(scale) } else { 0 };
    orb_w + if dots.is_empty() { 0 } else { dots_w(scale, dots) + px(SEP_DIP, scale) } + if icon { px(ICON_DIP, scale) + px(SEP_DIP, scale) } else { 0 }
}

// Ancho natural de la barra: margen, orbe, puntos, etiqueta opcional y reloj.
pub fn bar_width(scale: f32, orb: bool, icon: bool, dots: &[(usize, bool)], label_w: i32, clock_w: i32) -> i32 {
    let label = if label_w > 0 { label_w + px(SEP_DIP, scale) } else { 0 };
    px(PAD_DIP, scale) * 2 + lead_w(scale, orb, icon, dots) + label + clock_w
}

pub fn panel_height(scale: f32, shape: PanelShape) -> i32 {
    if shape.cal_rows > 0 {
        let fixed = CAL_HEAD_DIP + 2 + CAL_WEEK_DIP + 2 + SECTION_GAP_DIP + PANEL_TOP_DIP + PANEL_BOTTOM_DIP;
        return px(fixed, scale) + shape.cal_rows as i32 * px(CAL_CELL_DIP, scale) + shape.agenda as i32 * px(AGENDA_LINE_DIP, scale);
    }
    let gap = px(SECTION_GAP_DIP, scale);
    let media = if shape.media { px(MEDIA_DIP, scale) + gap } else { 0 };
    let weather = if shape.weather { px(WEATHER_DIP, scale) + gap } else { 0 };
    let meters = if shape.meters > 0 { shape.meters as i32 * px(METER_DIP, scale) + (shape.meters as i32 - 1) * px(METER_GAP_DIP, scale) + gap } else { 0 };
    px(PANEL_TOP_DIP, scale) + media + weather + meters + px(CHIP_H_DIP, scale) + px(PANEL_BOTTOM_DIP, scale)
}

// Radio de las esquinas de abajo; nunca mayor que la mitad de la altura.
pub fn corner(scale: f32, height: i32) -> i32 {
    px(CORNER_DIP, scale).min(height / 2)
}

// Suavizado de entrada/salida (smoothstep) para el fade del contenido.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// Mezcla un píxel RGB hacia `base` con factor `f` (1 = píxel intacto, 0 = solo fondo); no toca el alfa.
pub fn blend_toward(p: u32, base: (u8, u8, u8), f: f32) -> u32 {
    let ch = |shift: u32, b: u8| {
        let c = ((p >> shift) & 0xFF) as f32;
        ((b as f32 + (c - b as f32) * f).round().clamp(0.0, 255.0) as u32) << shift
    };
    (p & 0xFF00_0000) | ch(16, base.0) | ch(8, base.1) | ch(0, base.2)
}

// Ventanas de cada layout de komorebi como rectángulos (x, y, w, h) en 0..1; la primera es la principal.
pub fn layout_cells(name: &str) -> &'static [(f32, f32, f32, f32)] {
    match name {
        "Columns" => &[(0.0, 0.0, 0.34, 1.0), (0.34, 0.0, 0.33, 1.0), (0.67, 0.0, 0.33, 1.0)],
        "Rows" => &[(0.0, 0.0, 1.0, 0.34), (0.0, 0.34, 1.0, 0.33), (0.0, 0.67, 1.0, 0.33)],
        "VerticalStack" => &[(0.0, 0.0, 0.5, 1.0), (0.5, 0.0, 0.5, 0.5), (0.5, 0.5, 0.5, 0.5)],
        "RightMainVerticalStack" => &[(0.5, 0.0, 0.5, 1.0), (0.0, 0.0, 0.5, 0.5), (0.0, 0.5, 0.5, 0.5)],
        "HorizontalStack" => &[(0.0, 0.0, 1.0, 0.5), (0.0, 0.5, 0.5, 0.5), (0.5, 0.5, 0.5, 0.5)],
        "UltrawideVerticalStack" => &[(0.25, 0.0, 0.5, 1.0), (0.0, 0.0, 0.25, 0.5), (0.0, 0.5, 0.25, 0.5), (0.75, 0.0, 0.25, 1.0)],
        "Grid" => &[(0.0, 0.0, 0.5, 0.5), (0.5, 0.0, 0.5, 0.5), (0.0, 0.5, 0.5, 0.5), (0.5, 0.5, 0.5, 0.5)],
        _ => &[(0.0, 0.0, 0.5, 1.0), (0.5, 0.0, 0.5, 0.5), (0.5, 0.5, 0.25, 0.5), (0.75, 0.5, 0.25, 0.5)],
    }
}

// Cobertura 0..1 del notch: esquinas de arriba rectas (pegadas al borde), las de abajo redondeadas.
pub fn notch_coverage(x: f32, y: f32, w: f32, h: f32, r: f32, flip: bool) -> f32 {
    let y = if flip { h - y } else { y };
    if y <= h - r {
        return 1.0;
    }
    let cx = x.clamp(r, w - r);
    (r - ((x - cx).powi(2) + (y - (h - r)).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0)
}

impl Layout {
    pub fn new(i: &Inputs) -> Layout {
        let s = i.scale;
        let (pad, sep, bar) = (px(PAD_DIP, s), px(SEP_DIP, s), bar_h(s));
        let orb = R::new(pad / 2, 0, if i.orb { bar } else { 0 }, bar);
        let mut x = pad + if i.orb { bar } else { 0 };
        let dot_h = px(DOT_DIP, s);
        let dots = i
            .dots
            .iter()
            .map(|&(ws, focused)| {
                let w = px(if focused { DOT_ACTIVE_DIP } else { DOT_DIP }, s);
                let it = Item { r: R::new(x, (bar - dot_h) / 2, w, dot_h), ws, focused };
                x += w + px(GAP_DIP, s);
                it
            })
            .collect::<Vec<_>>();
        let lead_end = pad + lead_w(s, i.orb, i.icon, i.dots);
        let icon_side = px(ICON_DIP, s);
        let icon = if i.icon { R::new(lead_end - sep - icon_side, (bar - icon_side) / 2, icon_side, icon_side) } else { R::default() };
        let clock = R::new(i.width - pad - i.clock_w, 0, i.clock_w, bar);
        // El botón de pin solo existe con el panel abierto y le quita sitio a la etiqueta.
        let pin = if i.panel.is_some() { R::new(clock.x - sep - bar, 0, bar, bar) } else { R::default() };
        let label_end = if i.panel.is_some() { pin.x } else { clock.x } - sep;
        let label = R::new(lead_end, 0, label_end - lead_end, bar);
        let progress = R::new(pad, bar + px(2, s), i.width - 2 * pad, px(2, s));
        let panel = i.panel.map(|shape| Self::panel_rects(s, i, shape, bar));
        Layout { width: i.width, height: i.height, flip: i.flip, bar_h: bar, orb, icon, dots, label, clock, progress, pin, panel }
    }

    fn calendar_rects(s: f32, i: &Inputs, shape: PanelShape, bar: i32) -> PanelRects {
        let (pad, gap) = (px(PAD_DIP, s), px(SECTION_GAP_DIP, s));
        let right = i.width - pad;
        let (head, week, cell, line, btn) = (px(CAL_HEAD_DIP, s), px(CAL_WEEK_DIP, s), px(CAL_CELL_DIP, s), px(AGENDA_LINE_DIP, s), px(BTN_DIP, s));
        let col = ((right - pad) / 7).max(0);
        let x0 = pad + (right - pad - 7 * col) / 2;
        let mut y = bar + px(PANEL_TOP_DIP, s);
        let mut p = PanelRects { cal_title: R::new(pad, y, right - pad - 2 * btn, head), cal_prev: R::new(right - 2 * btn, y, btn, head), cal_next: R::new(right - btn, y, btn, head), ..Default::default() };
        y += head + 2;
        p.cal_heads = (0..7).map(|c| R::new(x0 + c * col, y, col, week)).collect();
        y += week + 2;
        p.cal_cells = (0..shape.cal_rows * 7).map(|n| R::new(x0 + (n % 7) as i32 * col, y + (n / 7) as i32 * cell, col, cell)).collect();
        y += shape.cal_rows as i32 * cell + gap;
        p.agenda = (0..shape.agenda).map(|n| R::new(pad, y + n as i32 * line, right - pad, line)).collect();
        if i.flip {
            // El orden visual (título arriba, agenda abajo) no cambia al pegar la isla abajo: se espeja dentro del panel.
            let total = panel_height(s, shape);
            for r in [&mut p.cal_title, &mut p.cal_prev, &mut p.cal_next].into_iter().chain(&mut p.cal_heads).chain(&mut p.cal_cells).chain(&mut p.agenda) {
                r.y = 2 * bar + total - r.y - r.h;
            }
        }
        p
    }

    fn panel_rects(s: f32, i: &Inputs, shape: PanelShape, bar: i32) -> PanelRects {
        if shape.cal_rows > 0 {
            return Self::calendar_rects(s, i, shape, bar);
        }
        let (pad, gap) = (px(PAD_DIP, s), px(SECTION_GAP_DIP, s));
        let right = i.width - pad;
        let mut y = bar + px(PANEL_TOP_DIP, s);
        let mut p = PanelRects::default();
        if shape.media {
            let (mh, bw) = (px(MEDIA_DIP, s), px(BTN_DIP, s));
            p.next = R::new(right - bw, y, bw, mh);
            p.toggle = R::new(right - 2 * bw, y, bw, mh);
            p.prev = R::new(right - 3 * bw, y, bw, mh);
            let text_w = p.prev.x - pad - px(SEP_DIP, s);
            p.title = R::new(pad, y, text_w, mh / 2);
            p.artist = R::new(pad, y + mh / 2, text_w, mh / 2);
            if i.flip {
                std::mem::swap(&mut p.title.y, &mut p.artist.y);
            }
            y += mh + gap;
        }
        if shape.weather {
            let (wh, line) = (px(WEATHER_DIP, s), px(14, s));
            let title_h = wh - 2 * line;
            p.weather = R::new(pad, y, right - pad, wh);
            p.weather_title = R::new(pad, y, right - pad - px(90, s), title_h);
            p.weather_range = R::new(right - px(90, s), y, px(90, s), title_h);
            p.weather_detail = R::new(pad, y + title_h, right - pad, line);
            p.weather_detail2 = R::new(pad, y + title_h + line, right - pad, line);
            if i.flip {
                std::mem::swap(&mut p.weather_title.y, &mut p.weather_detail2.y);
                p.weather_range.y = p.weather_title.y;
            }
            y += wh + gap;
        }
        let (mh, label_w, g) = (px(METER_DIP, s), px(34, s), px(METER_GAP_DIP, s));
        for _ in 0..shape.meters {
            let track_h = px(4, s);
            p.meters.push(Meter {
                label: R::new(pad, y, label_w, mh),
                track: R::new(pad + label_w + g, y + (mh - track_h) / 2, right - label_w - (pad + label_w + g) - g, track_h),
                value: R::new(right - label_w, y, label_w, mh),
            });
            y += mh + g;
        }
        if shape.meters > 0 {
            y += gap - g;
        }
        let (cw, ch, cg) = (px(CHIP_W_DIP, s), px(CHIP_H_DIP, s), px(CHIP_GAP_DIP, s));
        let mut x = pad;
        for &(ws, focused) in i.dots {
            p.chips.push(Item { r: R::new(x, y, cw, ch), ws, focused });
            x += cw + cg;
        }
        p.notes = R::new(x + gap - cg, y, right - (x + gap - cg), ch);
        p
    }

    // Convierte un rectángulo "con el borde arriba" a coordenadas de bitmap.
    pub fn place(&self, r: R) -> R {
        if self.flip { R { y: self.height - r.y - r.h, ..r } } else { r }
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<Hit> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return None;
        }
        let y = if self.flip { self.height - 1 - y } else { y };
        if self.pin.w > 0 && self.pin.contains(x, y) {
            return Some(Hit::Pin);
        }
        let reach = self.bar_h / 4;
        if self.icon.w > 0 && R::new(self.icon.x - reach, 0, self.icon.w + 2 * reach, self.bar_h).contains(x, y) {
            return Some(Hit::LayoutIcon);
        }
        if R::new(self.clock.x - reach, 0, self.clock.w + 2 * reach, self.bar_h).contains(x, y) {
            return Some(Hit::Clock);
        }
        if let Some(p) = &self.panel {
            for (r, h) in [(p.cal_prev, Hit::CalPrev), (p.cal_next, Hit::CalNext)] {
                if r.w > 0 && r.contains(x, y) {
                    return Some(h);
                }
            }
            if let Some(n) = p.cal_cells.iter().position(|c| c.contains(x, y)) {
                return Some(Hit::CalCell(n));
            }
            for (r, h) in [(p.prev, Hit::Prev), (p.toggle, Hit::Toggle), (p.next, Hit::Next)] {
                if r.contains(x, y) {
                    return Some(h);
                }
            }
            if let Some(c) = p.chips.iter().find(|c| c.r.contains(x, y)) {
                return Some(Hit::Workspace(c.ws));
            }
            if p.weather.w > 0 && p.weather.contains(x, y) {
                return Some(Hit::Weather);
            }
            for (n, m) in p.meters.iter().enumerate() {
                if R::new(m.label.x, m.label.y, m.value.x + m.value.w - m.label.x, m.label.h).contains(x, y) {
                    let on_track = x >= m.track.x && x < m.track.x + m.track.w;
                    return Some(Hit::Meter(n, on_track.then(|| (x - m.track.x) as f32 / m.track.w.max(1) as f32)));
                }
            }
        }
        let slack = self.dots.first().map_or(0, |d| d.r.h);
        self.dots
            .iter()
            .find(|d| R::new(d.r.x - slack / 2, 0, d.r.w + slack, self.bar_h).contains(x, y))
            .map(|d| Hit::Workspace(d.ws))
            .or(Some(Hit::Surface))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(flip: bool, panel: Option<PanelShape>, dots: &[(usize, bool)]) -> Layout {
        let width = bar_width(1.0, false, false, dots, 0, 40).max(PANEL_W_DIP);
        let height = bar_h(1.0) + panel.map_or(0, |p| panel_height(1.0, p));
        Layout::new(&Inputs { scale: 1.0, width, height, flip, clock_w: 40, orb: false, icon: false, dots, panel })
    }

    #[test]
    fn notch_is_square_on_top_and_round_below() {
        let (w, h, r) = (100.0, 40.0, 14.0);
        assert_eq!(notch_coverage(0.5, 0.5, w, h, r, false), 1.0);
        assert!(notch_coverage(0.5, h - 0.5, w, h, r, false) < 0.1);
        assert!(notch_coverage(0.5, 0.5, w, h, r, true) < 0.1);
        assert_eq!(notch_coverage(0.5, h - 0.5, w, h, r, true), 1.0);
    }

    #[test]
    fn fade_blends_between_background_and_pixel() {
        let base = (10, 20, 30);
        let p = 0x00C8_C8C8;
        assert_eq!(blend_toward(p, base, 1.0), p);
        assert_eq!(blend_toward(p, base, 0.0), 10 << 16 | 20 << 8 | 30);
        let half = blend_toward(p, base, 0.5);
        assert_eq!((half >> 16) & 0xFF, 105);
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert!(ease(0.25) < 0.25 && ease(0.75) > 0.75);
    }

    #[test]
    fn every_layout_icon_tiles_the_unit_square() {
        for name in ["BSP", "Columns", "Rows", "VerticalStack", "RightMainVerticalStack", "HorizontalStack", "UltrawideVerticalStack", "Grid", "desconocido"] {
            let cells = layout_cells(name);
            let area: f32 = cells.iter().map(|c| c.2 * c.3).sum();
            assert!((area - 1.0).abs() < 0.02, "{name}: área {area}");
            assert!(cells.iter().all(|c| c.0 >= 0.0 && c.1 >= 0.0 && c.0 + c.2 <= 1.001 && c.1 + c.3 <= 1.001), "{name} se sale del cuadrado");
        }
    }

    #[test]
    fn layout_icon_sits_after_the_dots_and_is_hit() {
        let dots = [(0, true), (1, false)];
        let width = bar_width(1.0, false, true, &dots, 0, 40);
        let l = Layout::new(&Inputs { scale: 1.0, width, height: bar_h(1.0), flip: false, clock_w: 40, orb: false, icon: true, dots: &dots, panel: None });
        assert!(l.icon.x >= l.dots[1].r.x + l.dots[1].r.w && l.icon.x + l.icon.w <= l.label.x + 1);
        assert_eq!(l.hit(l.icon.x + 1, l.icon.y + 1), Some(Hit::LayoutIcon));
        let none = Layout::new(&Inputs { scale: 1.0, width, height: bar_h(1.0), flip: false, clock_w: 40, orb: false, icon: false, dots: &dots, panel: None });
        assert_eq!(none.icon.w, 0);
    }

    #[test]
    fn dots_hit_their_workspace_and_the_rest_is_surface() {
        let l = inputs(false, None, &[(0, true), (2, false)]);
        let d = &l.dots[1];
        assert_eq!(l.hit(d.r.x + 1, 5), Some(Hit::Workspace(2)));
        assert_eq!(l.hit(l.clock.x + 1, 5), Some(Hit::Clock));
        assert_eq!(l.pin.w, 0, "sin panel no hay botón de pin");
        assert_eq!(l.hit(-1, 5), None);
    }

    #[test]
    fn panel_controls_and_chips_are_hit() {
        let shape = PanelShape { media: true, weather: true, meters: 3, ..Default::default() };
        let l = inputs(false, Some(shape), &[(0, true), (1, false)]);
        let p = l.panel.as_ref().unwrap();
        assert_eq!(l.hit(p.toggle.x + 2, p.toggle.y + 2), Some(Hit::Toggle));
        assert_eq!(l.hit(p.prev.x + 2, p.prev.y + 2), Some(Hit::Prev));
        assert_eq!(l.hit(p.next.x + 2, p.next.y + 2), Some(Hit::Next));
        let c = &p.chips[1];
        assert_eq!(l.hit(c.r.x + 2, c.r.y + 2), Some(Hit::Workspace(1)));
        assert_eq!(l.hit(p.title.x + 2, p.title.y + 2), Some(Hit::Surface));
        assert_eq!(l.hit(l.pin.x + 2, l.pin.y + 2), Some(Hit::Pin));
        assert_eq!(l.hit(p.weather.x + 2, p.weather.y + 2), Some(Hit::Weather));
        let m = &p.meters[1];
        assert_eq!(l.hit(m.label.x + 1, m.label.y + 1), Some(Hit::Meter(1, None)));
        assert_eq!(l.hit(m.track.x, m.track.y), Some(Hit::Meter(1, Some(0.0))));
        match l.hit(m.track.x + m.track.w / 2, m.track.y) {
            Some(Hit::Meter(1, Some(f))) => assert!((f - 0.5).abs() < 0.02),
            other => panic!("{other:?}"),
        }
        assert!(l.pin.x + l.pin.w <= l.clock.x && l.label.x + l.label.w <= l.pin.x);
    }

    #[test]
    fn flip_mirrors_hit_testing_and_placement() {
        let shape = PanelShape { media: true, weather: true, meters: 2, ..Default::default() };
        let l = inputs(true, Some(shape), &[(0, true)]);
        let p = l.panel.as_ref().unwrap();
        let placed = l.place(p.toggle);
        assert_eq!(l.hit(placed.x + 2, placed.y + 2), Some(Hit::Toggle));
        assert!(l.place(R::new(0, 0, 1, l.bar_h)).y > l.place(p.toggle).y);
    }

    #[test]
    fn everything_stays_inside_the_box_at_any_scale() {
        for scale in [1.0, 1.5, 2.0] {
            let dots = [(0, false), (1, true), (2, false)];
            let shape = Some(PanelShape { media: true, weather: true, meters: 3, ..Default::default() });
            let width = px(PANEL_W_DIP, scale);
            let height = bar_h(scale) + panel_height(scale, shape.unwrap());
            let l = Layout::new(&Inputs { scale, width, height, flip: false, clock_w: px(40, scale), orb: true, icon: false, dots: &dots, panel: shape });
            let p = l.panel.as_ref().unwrap();
            let rects = [p.title, p.artist, p.weather, p.weather_title, p.weather_range, p.weather_detail, p.weather_detail2, p.prev, p.toggle, p.next, p.notes, l.clock].into_iter().chain(p.chips.iter().map(|c| c.r)).chain(p.meters.iter().flat_map(|m| [m.label, m.track, m.value]));
            for r in rects {
                assert!(r.x >= 0 && r.w >= 0 && r.x + r.w <= width && r.y >= 0 && r.y + r.h <= height, "{r:?} fuera de {width}x{height} a {scale}");
            }
            assert!(l.label.x + l.label.w <= l.clock.x, "la etiqueta pisa el reloj a {scale}");
        }
    }

    #[test]
    fn calendar_panel_hits_cells_and_stays_inside_in_both_orientations() {
        for (scale, flip) in [(1.0, false), (1.0, true), (1.5, true)] {
            let shape = PanelShape { cal_rows: 5, agenda: 3, ..Default::default() };
            let width = px(PANEL_W_DIP, scale);
            let height = bar_h(scale) + panel_height(scale, shape);
            let l = Layout::new(&Inputs { scale, width, height, flip, clock_w: px(40, scale), orb: false, icon: false, dots: &[(0, true)], panel: Some(shape) });
            let p = l.panel.as_ref().unwrap();
            assert_eq!((p.cal_cells.len(), p.cal_heads.len(), p.agenda.len()), (35, 7, 3));
            for (n, c) in p.cal_cells.iter().enumerate() {
                let r = l.place(*c);
                assert_eq!(l.hit(r.x + 2, r.y + 2), Some(Hit::CalCell(n)), "celda {n} flip={flip}");
            }
            let n = l.place(p.cal_next);
            assert_eq!(l.hit(n.x + 2, n.y + 2), Some(Hit::CalNext));
            let pv = l.place(p.cal_prev);
            assert_eq!(l.hit(pv.x + 2, pv.y + 2), Some(Hit::CalPrev));
            for r in [p.cal_title, p.cal_prev, p.cal_next].into_iter().chain(p.cal_heads.iter().copied()).chain(p.cal_cells.iter().copied()).chain(p.agenda.iter().copied()) {
                let r = l.place(r);
                assert!(r.x >= 0 && r.x + r.w <= width && r.y >= 0 && r.y + r.h <= height, "{r:?} fuera de {width}x{height}");
            }
            let (first, last) = (l.place(p.cal_title), l.place(*p.agenda.last().unwrap()));
            assert!(first.y < last.y, "el título queda arriba de la agenda en flip={flip}");
        }
    }

    #[test]
    fn panel_grows_with_its_sections() {
        let base = panel_height(1.0, PanelShape { media: false, weather: false, meters: 0, ..Default::default() });
        assert!(panel_height(1.0, PanelShape { media: true, weather: false, meters: 0, ..Default::default() }) > base);
        assert!(panel_height(1.0, PanelShape { media: false, weather: false, meters: 2, ..Default::default() }) > base);
        assert!(panel_height(1.0, PanelShape { media: false, weather: true, meters: 0, ..Default::default() }) > base);
    }
}
