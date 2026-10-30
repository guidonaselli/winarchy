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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelShape {
    pub media: bool,
    pub weather: bool,
    pub meters: usize,
}

pub struct Inputs<'a> {
    pub scale: f32,
    pub width: i32,
    pub height: i32,
    pub flip: bool,
    pub clock_w: i32,
    pub orb: bool,
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

pub fn progress_band(scale: f32) -> i32 {
    px(PROGRESS_BAND_DIP, scale)
}

fn dots_w(scale: f32, dots: &[(usize, bool)]) -> i32 {
    let w: i32 = dots.iter().map(|d| px(if d.1 { DOT_ACTIVE_DIP } else { DOT_DIP }, scale)).sum();
    w + px(GAP_DIP, scale) * (dots.len() as i32 - 1).max(0)
}

fn lead_w(scale: f32, orb: bool, dots: &[(usize, bool)]) -> i32 {
    let orb_w = if orb { bar_h(scale) } else { 0 };
    orb_w + if dots.is_empty() { 0 } else { dots_w(scale, dots) + px(SEP_DIP, scale) }
}

// Ancho natural de la barra: margen, orbe, puntos, etiqueta opcional y reloj.
pub fn bar_width(scale: f32, orb: bool, dots: &[(usize, bool)], label_w: i32, clock_w: i32) -> i32 {
    let label = if label_w > 0 { label_w + px(SEP_DIP, scale) } else { 0 };
    px(PAD_DIP, scale) * 2 + lead_w(scale, orb, dots) + label + clock_w
}

pub fn panel_height(scale: f32, shape: PanelShape) -> i32 {
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
        let lead_end = pad + lead_w(s, i.orb, i.dots);
        let clock = R::new(i.width - pad - i.clock_w, 0, i.clock_w, bar);
        // El botón de pin solo existe con el panel abierto y le quita sitio a la etiqueta.
        let pin = if i.panel.is_some() { R::new(clock.x - sep - bar, 0, bar, bar) } else { R::default() };
        let label_end = if i.panel.is_some() { pin.x } else { clock.x } - sep;
        let label = R::new(lead_end, 0, label_end - lead_end, bar);
        let progress = R::new(pad, bar + px(2, s), i.width - 2 * pad, px(2, s));
        let panel = i.panel.map(|shape| Self::panel_rects(s, i, shape, bar));
        Layout { width: i.width, height: i.height, flip: i.flip, bar_h: bar, orb, dots, label, clock, progress, pin, panel }
    }

    fn panel_rects(s: f32, i: &Inputs, shape: PanelShape, bar: i32) -> PanelRects {
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
        if let Some(p) = &self.panel {
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
        let width = bar_width(1.0, false, dots, 0, 40).max(PANEL_W_DIP);
        let height = bar_h(1.0) + panel.map_or(0, |p| panel_height(1.0, p));
        Layout::new(&Inputs { scale: 1.0, width, height, flip, clock_w: 40, orb: false, dots, panel })
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
    fn dots_hit_their_workspace_and_the_rest_is_surface() {
        let l = inputs(false, None, &[(0, true), (2, false)]);
        let d = &l.dots[1];
        assert_eq!(l.hit(d.r.x + 1, 5), Some(Hit::Workspace(2)));
        assert_eq!(l.hit(l.clock.x + 1, 5), Some(Hit::Surface));
        assert_eq!(l.pin.w, 0, "sin panel no hay botón de pin");
        assert_eq!(l.hit(-1, 5), None);
    }

    #[test]
    fn panel_controls_and_chips_are_hit() {
        let shape = PanelShape { media: true, weather: true, meters: 3 };
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
        let shape = PanelShape { media: true, weather: true, meters: 2 };
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
            let shape = Some(PanelShape { media: true, weather: true, meters: 3 });
            let width = px(PANEL_W_DIP, scale);
            let height = bar_h(scale) + panel_height(scale, shape.unwrap());
            let l = Layout::new(&Inputs { scale, width, height, flip: false, clock_w: px(40, scale), orb: true, dots: &dots, panel: shape });
            let p = l.panel.as_ref().unwrap();
            let rects = [p.title, p.artist, p.weather, p.weather_title, p.weather_range, p.weather_detail, p.weather_detail2, p.prev, p.toggle, p.next, p.notes, l.clock].into_iter().chain(p.chips.iter().map(|c| c.r)).chain(p.meters.iter().flat_map(|m| [m.label, m.track, m.value]));
            for r in rects {
                assert!(r.x >= 0 && r.w >= 0 && r.x + r.w <= width && r.y >= 0 && r.y + r.h <= height, "{r:?} fuera de {width}x{height} a {scale}");
            }
            assert!(l.label.x + l.label.w <= l.clock.x, "la etiqueta pisa el reloj a {scale}");
        }
    }

    #[test]
    fn panel_grows_with_its_sections() {
        let base = panel_height(1.0, PanelShape { media: false, weather: false, meters: 0 });
        assert!(panel_height(1.0, PanelShape { media: true, weather: false, meters: 0 }) > base);
        assert!(panel_height(1.0, PanelShape { media: false, weather: false, meters: 2 }) > base);
        assert!(panel_height(1.0, PanelShape { media: false, weather: true, meters: 0 }) > base);
    }
}
