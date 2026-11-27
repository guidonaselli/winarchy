use super::*;

#[test]
fn the_grid_wraps_at_six_columns_and_leaves_a_row_for_the_tooltip() {
    let g = grid(7, 1.0);
    assert_eq!(g.cells.len(), 7);
    assert_eq!((g.cells[5].x, g.cells[5].y), (8 + 5 * 40, 8));
    assert_eq!((g.cells[6].x, g.cells[6].y), (8, 8 + 40));
    assert_eq!(g.width, 2 * 8 + 6 * 40);
    assert_eq!(g.height, 2 * 8 + 2 * 40 + 26);
    assert_eq!(g.tip.y + g.tip.h + 8, g.height);
}

#[test]
fn a_short_row_is_only_as_wide_as_its_icons() {
    assert_eq!(grid(2, 1.0).width, 2 * 8 + 2 * 40);
    assert_eq!(grid(0, 1.0).cells.len(), 0);
}

#[test]
fn the_flyout_sits_above_a_bottom_dock_and_stays_inside_the_monitor() {
    let mon = (0, 0, 1920, 1080);
    let anchor = R { x: 1000, y: 1020, w: 40, h: 40 };
    assert_eq!(place(anchor, mon, true, (200, 100), 1.0), (920, 1020 - MARGIN_DIP - 100));
    assert_eq!(place(R { x: 1890, ..anchor }, mon, true, (200, 100), 1.0).0, 1920 - MARGIN_DIP - 200);
    assert_eq!(place(R { x: 0, ..anchor }, mon, true, (200, 100), 1.0).0, MARGIN_DIP);
    assert_eq!(place(R { y: 8, ..anchor }, mon, false, (200, 100), 1.0).1, 8 + 40 + MARGIN_DIP);
}

#[test]
fn a_tall_flyout_never_leaves_the_monitor_vertically() {
    let mon = (0, 0, 1920, 1080);
    let anchor = R { x: 1000, y: 1020, w: 40, h: 40 };
    assert_eq!(place(anchor, mon, true, (200, 2000), 1.0).1, MARGIN_DIP);
    let top = R { y: 8, ..anchor };
    assert_eq!(place(top, mon, false, (200, 2000), 1.0).1, MARGIN_DIP);
    assert_eq!(place(top, (0, 0, 1920, 90), false, (200, 100), 1.0).1, MARGIN_DIP);
}

#[test]
fn render_cache_key_tracks_every_theme_token_used_by_the_flyout() {
    fn assert_theme_change(change: impl FnOnce(&mut Theme)) {
        let mut theme = Theme::default();
        let before = theme_render_key(&theme);
        change(&mut theme);
        assert_ne!(theme_render_key(&theme), before);
    }

    assert_theme_change(|t| t.background.push('x'));
    assert_theme_change(|t| t.foreground.push('x'));
    assert_theme_change(|t| t.accent.push('x'));
    assert_theme_change(|t| t.muted.push('x'));
    assert_theme_change(|t| t.font_family.push('x'));
    assert_theme_change(|t| t.font_size += 1);
    assert_theme_change(|t| t.dock_radius += 1);
    assert_theme_change(|t| t.opacity = 0.5);
    assert_theme_change(|t| t.dock_opacity = Some(0.5));
}

#[test]
fn scaling_keeps_color_and_premultiplies_alpha() {
    let opaque = vec![10, 20, 30, 255].repeat(4);
    assert!(scaled(&opaque, (2, 2), 1).iter().all(|p| *p == 0xFF_1E_14_0A), "opaque stays exact (BGRA 10,20,30)");
    let half = vec![200, 100, 0, 128].repeat(4);
    let p = scaled(&half, (2, 2), 1)[0];
    assert_eq!(p >> 24, 128);
    assert_eq!(p & 0xFF, 100, "blue 200 * 128/255");
    assert_eq!(scaled(&vec![0; 4], (1, 1), 3).len(), 9);
    assert!(scaled(&vec![0; 4], (1, 1), 3).iter().all(|p| *p == 0));
}
