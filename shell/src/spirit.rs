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
            Self::Listening => "Escuchando",
            Self::Thinking => "Pensando",
            Self::Speaking => "Hablando",
            Self::Error => "Error",
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

/// Círculo (o anillo si `ring` > 0) en coordenadas de la caja del orbe: centro (0,0), radio de la caja 1.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub alpha: f32,
    pub ring: f32,
    pub error: bool,
}

const TAU: f32 = std::f32::consts::TAU;

pub fn shapes(a: Assist, t: f32) -> Vec<Shape> {
    let dot = |x: f32, y: f32, r: f32, alpha: f32| Shape { x, y, r, alpha, ring: 0.0, error: false };
    match a {
        Assist::Idle => vec![],
        Assist::Listening => {
            let phase = (t * 0.9).fract();
            vec![
                dot(0.0, 0.0, 0.42 * (1.0 + 0.12 * (TAU * 1.6 * t).sin()), 1.0),
                Shape { x: 0.0, y: 0.0, r: 0.5 + 0.45 * phase, alpha: 0.6 * (1.0 - phase), ring: 0.09, error: false },
            ]
        }
        Assist::Thinking => {
            let mut v = vec![dot(0.0, 0.0, 0.26, 0.55)];
            for k in 0..3 {
                let ang = TAU * (0.8 * t + k as f32 / 3.0);
                v.push(dot(0.62 * ang.cos(), 0.62 * ang.sin(), 0.16, 1.0));
            }
            v
        }
        Assist::Speaking => {
            let amp = (TAU * 2.2 * t).sin().abs() * (0.6 + 0.4 * (TAU * 0.7 * t).sin());
            vec![dot(0.0, 0.0, 0.38 * (1.0 + 0.3 * amp), 1.0)]
        }
        Assist::Error => {
            let shake = (TAU * 12.0 * t).sin() * 0.12 * (1.0 - t / 2.5).max(0.0);
            vec![Shape { x: shake, y: 0.0, r: 0.42, alpha: 1.0, ring: 0.0, error: true }]
        }
    }
}

/// Pinta las formas sobre un buffer BGRA opaco dentro de la caja cuadrada (x0, 0, size, size).
pub fn paint(pixels: &mut [u32], width: i32, x0: i32, size: i32, shapes: &[Shape], accent: (u8, u8, u8)) {
    let half = size as f32 / 2.0;
    for s in shapes {
        let c = if s.error { (255u8, 95u8, 95u8) } else { accent };
        let (cx, cy, rad) = (x0 as f32 + half + s.x * half, half + s.y * half, s.r * half);
        let (lo, hi) = ((cx - rad - 2.0).floor() as i32, (cx + rad + 2.0).ceil() as i32);
        for y in 0..size {
            for x in lo.max(x0)..hi.min(x0 + size) {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let cov = if s.ring > 0.0 { (s.ring * half * 0.5 - (d - rad).abs() + 0.5).clamp(0.0, 1.0) } else { (rad - d + 0.5).clamp(0.0, 1.0) } * s.alpha;
                if cov > 0.0 {
                    let i = (y * width + x) as usize;
                    let (pr, pg, pb) = ((pixels[i] >> 16) as u8, (pixels[i] >> 8) as u8, pixels[i] as u8);
                    let mix = |b: u8, f: u8| (b as f32 * (1.0 - cov) + f as f32 * cov) as u32;
                    pixels[i] = 0xFF00_0000 | mix(pr, c.0) << 16 | mix(pg, c.1) << 8 | mix(pb, c.2);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_stays_inside_the_box_over_time() {
        for a in [Assist::Listening, Assist::Thinking, Assist::Speaking, Assist::Error] {
            for i in 0..300 {
                for s in shapes(a, i as f32 * 0.01) {
                    assert!(s.x.abs() + s.r <= 1.001 && s.y.abs() + s.r <= 1.001, "{a:?} t={i}: {s:?}");
                }
            }
        }
        assert!(shapes(Assist::Idle, 1.0).is_empty());
    }

    #[test]
    fn paint_only_touches_the_orb_box() {
        let (w, size) = (60, 20);
        let mut px = vec![0xFF10_1010u32; (w * size) as usize];
        paint(&mut px, w, 10, size, &shapes(Assist::Speaking, 0.1), (200, 100, 50));
        assert!(px.iter().enumerate().any(|(i, p)| *p != 0xFF10_1010 && (i as i32 % w) >= 10 && (i as i32 % w) < 30));
        assert!(px.iter().enumerate().all(|(i, p)| *p == 0xFF10_1010 || ((i as i32 % w) >= 10 && (i as i32 % w) < 30)));
    }

    #[test]
    fn parses_known_states_only() {
        assert_eq!(Assist::parse("thinking"), Some(Assist::Thinking));
        assert_eq!(Assist::parse("dancing"), None);
    }
}
