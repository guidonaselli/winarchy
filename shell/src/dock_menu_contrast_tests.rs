use super::close_glyph_color;

#[test]
fn tokyo_night_highlight_uses_a_dark_close_glyph() {
    assert_eq!(close_glyph_color((0x7a, 0xa2, 0xf7)), (0, 0, 0));
}

#[test]
fn latte_highlight_uses_a_light_close_glyph() {
    assert_eq!(close_glyph_color((0x1e, 0x66, 0xf5)), (255, 255, 255));
}

#[test]
fn close_glyph_switches_to_dark_at_the_contrast_crossover() {
    assert_eq!(
        [close_glyph_color((117, 117, 117)), close_glyph_color((118, 118, 118))],
        [(255, 255, 255), (0, 0, 0)],
    );
}

#[test]
fn close_glyph_contrasts_with_black_and_white_highlights() {
    assert_eq!(
        [close_glyph_color((0, 0, 0)), close_glyph_color((255, 255, 255))],
        [(255, 255, 255), (0, 0, 0)],
    );
}

fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let linear = |c: u8| {
        let s = f64::from(c) / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

#[test]
fn highlighted_close_glyph_meets_four_point_five_to_one_contrast() {
    for accent in [
        (0x7a, 0xa2, 0xf7),
        (0x1e, 0x66, 0xf5),
        (0x20, 0x5e, 0xa6),
        (0x6e, 0x6e, 0x6e),
        (117, 117, 117),
        (118, 118, 118),
        (0, 0, 0),
        (255, 255, 255),
    ] {
        let background = luminance(accent);
        let glyph = luminance(close_glyph_color(accent));
        let ratio = (background.max(glyph) + 0.05) / (background.min(glyph) + 0.05);
        assert!(ratio >= 4.5, "accent {accent:?}: contrast {ratio}:1");
    }
}
