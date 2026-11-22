use crate::icons::{Icon, layers};
use crate::style::ICON_TINT_OPACITY;
use std::path::PathBuf;

const FALLBACK: [[u8; 3]; 2] = [[0xe6, 0xe6, 0xe6], [0xf3, 0x8d, 0x70]];

fn hex(color: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

// SvgImageSource only loads files, so each icon is written once per color pair and reused.
pub fn file(icon: Icon, colors: Option<[[u8; 3]; 2]>) -> Option<PathBuf> {
    let [foreground, accent] = colors.unwrap_or(FALLBACK);
    let folder = std::env::var_os("LOCALAPPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("winarchy-files")
        .join("icons")
        .join(format!("{}-{}-{}", hex(foreground), hex(accent), (ICON_TINT_OPACITY * 100.0) as u32));
    let path = folder.join(format!("{icon:?}.svg"));
    if !path.exists() {
        let body: String = layers(icon)
            .iter()
            .map(|(data, tint)| {
                if *tint {
                    format!(r##"<path d="{data}" fill="#{}" fill-opacity="{ICON_TINT_OPACITY}"/>"##, hex(accent))
                } else {
                    format!(r##"<path d="{data}" fill="#{}"/>"##, hex(foreground))
                }
            })
            .collect();
        std::fs::create_dir_all(&folder).ok()?;
        let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">{body}</svg>"#);
        std::fs::write(&path, svg).ok()?;
    }
    Some(path)
}
