use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Assist {
    Idle,
    Listening,
    Thinking,
    Speaking,
    Error,
}

impl Assist {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "idle" => Self::Idle,
            "listening" => Self::Listening,
            "thinking" => Self::Thinking,
            "speaking" => Self::Speaking,
            "error" => Self::Error,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "",
            Self::Listening => crate::i18n::t(crate::i18n::K::Listening),
            Self::Thinking => crate::i18n::t(crate::i18n::K::Thinking),
            Self::Speaking => crate::i18n::t(crate::i18n::K::Speaking),
            Self::Error => crate::i18n::t(crate::i18n::K::Error),
        }
    }
}

// Un asistente que murió a mitad de turno no puede dejar la isla ocupada para siempre.
const ERROR_SHOWN: Duration = Duration::from_millis(2500);
const STALE: Duration = Duration::from_secs(90);

static STATE: Mutex<Option<(Assist, Instant)>> = Mutex::new(None);

pub fn set(a: Assist) {
    *STATE.lock().unwrap() = if a == Assist::Idle { None } else { Some((a, Instant::now())) };
}

/// Estado vigente y segundos desde que empezó; vence solo el error y lo que quedó colgado.
pub fn current() -> (Assist, f32) {
    let mut g = STATE.lock().unwrap();
    match *g {
        Some((a, t)) => {
            let age = t.elapsed();
            if age > STALE || (a == Assist::Error && age > ERROR_SHOWN) {
                *g = None;
                (Assist::Idle, 0.0)
            } else {
                (a, age.as_secs_f32())
            }
        }
        None => (Assist::Idle, 0.0),
    }
}

const TAU: f32 = std::f32::consts::TAU;

type P = (f32, f32);

/// Distancia con signo (aprox.) a una elipse.
fn ellipse(p: P, c: P, r: P) -> f32 {
    let (qx, qy) = ((p.0 - c.0) / r.0, (p.1 - c.1) / r.1);
    ((qx * qx + qy * qy).sqrt() - 1.0) * r.0.min(r.1)
}

/// Distancia con signo exacta a un triángulo.
fn tri(p: P, v: [P; 3]) -> f32 {
    let e = [(v[1].0 - v[0].0, v[1].1 - v[0].1), (v[2].0 - v[1].0, v[2].1 - v[1].1), (v[0].0 - v[2].0, v[0].1 - v[2].1)];
    let cross = |a: P, b: P| a.0 * b.1 - a.1 * b.0;
    let s = cross(e[0], (v[2].0 - v[0].0, v[2].1 - v[0].1)).signum();
    let (mut d, mut inside) = (f32::MAX, true);
    for i in 0..3 {
        let w = (p.0 - v[i].0, p.1 - v[i].1);
        let t = ((w.0 * e[i].0 + w.1 * e[i].1) / (e[i].0 * e[i].0 + e[i].1 * e[i].1)).clamp(0.0, 1.0);
        let q = (w.0 - e[i].0 * t, w.1 - e[i].1 * t);
        d = d.min(q.0 * q.0 + q.1 * q.1);
        inside &= s * cross(e[i], w) >= 0.0;
    }
    if inside {
        -d.sqrt()
    } else {
        d.sqrt()
    }
}

/// Silueta de Espectro: cabeza redonda con orejas, aletas y cola en punta; la oreja derecha es la más alta.
fn body(p: P) -> f32 {
    [
        ellipse(p, (0.0, 0.18), (0.72, 0.55)),
        tri(p, [(-0.68, -0.08), (-0.60, -0.84), (-0.14, -0.40)]),
        tri(p, [(0.12, -0.42), (0.50, -0.88), (0.70, -0.06)]),
        tri(p, [(-0.62, 0.28), (-0.92, 0.12), (-0.60, 0.55)]),
        tri(p, [(0.62, 0.30), (0.93, 0.18), (0.60, 0.56)]),
        tri(p, [(0.30, 0.62), (0.62, 0.90), (0.66, 0.50)]),
    ]
    .into_iter()
    .fold(f32::MAX, f32::min)
}

/// Cómo está la cara en un instante: ojos, párpado, boca y cuánto se mueve el cuerpo.
struct Face {
    rx: f32,
    ry: f32,
    lid: f32,
    eye: P,
    glow: f32,
    smile: bool,
    mouth: f32,
    shift: P,
    error: bool,
}

/// `look` es hacia dónde está el mouse respecto de la criatura, cada eje entre -1 y 1.
fn face(a: Assist, t: f32, look: P) -> Face {
    let ph = t % 4.2;
    let blink = if ph > 4.08 { 1.0 - ((ph - 4.08) / 0.06 - 1.0).abs().min(1.0) } else { 0.0 };
    let bob = 0.025 * (TAU * 0.8 * t).sin();
    let base = |ry: f32, lid: f32, glow: f32| Face {
        rx: 0.13,
        ry,
        lid: lid.max(blink * 0.9),
        eye: (look.0 * 0.08, look.1 * 0.06),
        glow,
        smile: false,
        mouth: 0.0,
        shift: (look.0 * 0.02, bob + look.1 * 0.015),
        error: false,
    };
    match a {
        Assist::Idle | Assist::Listening => Face { smile: true, ..base(0.22, 0.0, 0.35 + 0.15 * (TAU * 1.6 * t).sin()) },
        Assist::Thinking => Face { eye: (0.09 * (TAU * 0.7 * t).sin(), -0.07), ..base(0.19, 0.25, 0.25) },
        Assist::Speaking => Face { mouth: (TAU * 2.2 * t).sin().abs() * (0.6 + 0.4 * (TAU * 0.7 * t).sin()), ..base(0.2, 0.0, 0.3) },
        Assist::Error => {
            let shake = (TAU * 12.0 * t).sin() * 0.03 * (1.0 - t / 2.5).max(0.0);
            Face { rx: 0.15, mouth: 0.35, shift: (shake, bob), error: true, ..base(0.29, 0.0, 0.5) }
        }
    }
}

fn blend(px: &mut u32, c: (u8, u8, u8), a: f32) {
    if a <= 0.0 {
        return;
    }
    let mix = |b: u32, f: u8| (b as f32 * (1.0 - a) + f as f32 * a) as u32;
    *px = 0xFF00_0000 | mix(*px >> 16 & 0xFF, c.0) << 16 | mix(*px >> 8 & 0xFF, c.1) << 8 | mix(*px & 0xFF, c.2);
}

/// Pinta a Espectro sobre un buffer opaco dentro de la caja cuadrada (x0, 0, size, size).
#[allow(clippy::too_many_arguments)]
pub fn paint(pixels: &mut [u32], width: i32, x0: i32, size: i32, a: Assist, t: f32, look: P, accent: (u8, u8, u8)) {
    const SCALE: f32 = 0.86;
    const NAVY: (u8, u8, u8) = (20, 24, 66);
    const EYE: (u8, u8, u8) = (235, 240, 255);
    let (half, f) = (size as f32 / 2.0, face(a, t, look));
    let tint = if f.error { (255, 95, 95) } else { accent };
    let rim = (size as f32 * 0.04).max(1.0);
    let eyes = [-0.27, 0.27].map(|dx| (dx * SCALE + f.eye.0, 0.12 * SCALE + f.eye.1));
    let er = (f.rx * SCALE, f.ry * SCALE);
    let (mouth_c, smile_c) = ((0.0, 0.40 * SCALE), (0.0, 0.33 * SCALE));
    for y in 0..size {
        for x in x0..x0 + size {
            let u = (((x - x0) as f32 + 0.5) / half - 1.0, (y as f32 + 0.5) / half - 1.0);
            let p = ((u.0 - f.shift.0) / SCALE, (u.1 - f.shift.1) / SCALE);
            let d = body(p) * SCALE * half;
            if 0.5 - (d - rim) <= 0.0 {
                continue;
            }
            let px = &mut pixels[(y * width + x) as usize];
            blend(px, tint, (0.5 - (d - rim)).clamp(0.0, 1.0) * 0.85);
            blend(px, NAVY, (0.5 - d).clamp(0.0, 1.0));
            for c in eyes {
                let de = ellipse(u, c, er) * half;
                let cut = ((u.1 - (c.1 - er.1 + f.lid * 2.0 * er.1)) * half + 0.5).clamp(0.0, 1.0);
                if de > 0.0 {
                    blend(px, tint, f.glow * (1.0 - de / (0.22 * half)).clamp(0.0, 1.0) * cut);
                } else {
                    blend(px, EYE, (0.5 - de).clamp(0.0, 1.0) * cut);
                }
            }
            if f.smile && u.1 >= smile_c.1 {
                let ring = (0.035 * half - ellipse(u, smile_c, (0.10 * SCALE, 0.10 * SCALE)).abs() * half + 0.5).clamp(0.0, 1.0);
                blend(px, tint, ring * 0.9);
            }
            if f.mouth > 0.0 {
                blend(px, tint, (0.5 - ellipse(u, mouth_c, (0.09 * SCALE, (0.03 + 0.08 * f.mouth) * SCALE)) * half).clamp(0.0, 1.0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: u32 = 0xFF10_1010;
    const ALL: [Assist; 4] = [Assist::Listening, Assist::Thinking, Assist::Speaking, Assist::Error];

    fn frame(a: Assist, t: f32, look: P) -> Vec<u32> {
        let (w, size) = (60, 24);
        let mut px = vec![BG; (w * size) as usize];
        paint(&mut px, w, 10, size, a, t, look, (94, 123, 255));
        px
    }

    #[test]
    fn paints_inside_the_box_and_never_touches_its_border() {
        for a in ALL {
            for i in 0..300 {
                let px = frame(a, i as f32 * 0.01, [(-1.0, -1.0), (1.0, 1.0)][i % 2]);
                assert!(px.iter().any(|p| *p != BG), "{a:?} t={i}: nothing drawn");
                for (n, p) in px.iter().enumerate() {
                    let (x, y) = (n as i32 % 60, n as i32 / 60);
                    let border = x == 10 || x == 33 || y == 0 || y == 23 || !(10..34).contains(&x);
                    assert!(*p == BG || !border, "{a:?} t={i}: pixel ({x},{y}) touched the border");
                }
            }
        }
    }

    #[test]
    fn eyes_follow_the_mouse() {
        let center = |look: P| -> f32 {
            let px = frame(Assist::Listening, 0.3, look);
            let eyes: Vec<f32> = px.iter().enumerate().filter(|(_, p)| (**p >> 16 & 0xFF) > 225 && (**p & 0xFF) > 240).map(|(i, _)| (i as i32 % 60) as f32).collect();
            assert!(!eyes.is_empty());
            eyes.iter().sum::<f32>() / eyes.len() as f32
        };
        assert!(center((1.0, 0.0)) > center((-1.0, 0.0)), "eyes did not move toward the cursor");
    }

    #[test]
    fn parses_known_states_only() {
        assert_eq!(Assist::parse("thinking"), Some(Assist::Thinking));
        assert_eq!(Assist::parse("dancing"), None);
    }
}
