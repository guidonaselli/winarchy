use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize, Clone)]
pub struct Theme {
    pub background: String,
    pub opacity: f32,
    pub foreground: String,
    pub accent: String,
    pub muted: String,
    pub font_family: String,
    pub font_size: i32,
    pub position: String,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            background: "#1a1b26".into(),
            opacity: 1.0,
            foreground: "#c0caf5".into(),
            accent: "#7aa2f7".into(),
            muted: "#565f89".into(),
            font_family: "Segoe UI".into(),
            font_size: 13,
            position: "top".into(),
        }
    }
}

impl Theme {
    pub fn load(path: &Path) -> Option<Theme> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }
}

pub fn rgb(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok()).unwrap_or(0);
    (p(0), p(2), p(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_falls_back_to_zero() {
        assert_eq!(rgb("#7aa2f7"), (0x7a, 0xa2, 0xf7));
        assert_eq!(rgb("zz"), (0, 0, 0));
    }

    #[test]
    fn loads_the_generated_file_shape() {
        let json = r##"{"_managed":"x","v":1,"background":"#000000","opacity":0.72,"foreground":"#ffffff","accent":"#ff0000","muted":"#111111","font_family":"JetBrainsMono Nerd Font","font_size":12,"position":"top"}"##;
        let t: Theme = serde_json::from_str(json).unwrap();
        assert_eq!(t.font_size, 12);
        assert_eq!(t.position, "top");
    }
}
