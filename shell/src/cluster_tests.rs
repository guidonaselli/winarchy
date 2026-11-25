use super::*;

#[test]
fn the_pill_grows_with_battery_and_notification_badge() {
    let base = shape(1.0, false, false, 0, 48);
    let full = shape(1.0, true, true, 0, 48);
    assert!(full.status.w > base.status.w && full.clock.w > base.clock.w);
    assert_eq!(base.width, base.desktop.x + base.desktop.w + 4);
    assert!(base.clock.x + base.clock.w <= base.desktop.x);
    assert!(base.status.x + base.status.w <= base.clock.x);
}

#[test]
fn buttons_cover_the_full_height_and_never_overlap() {
    let s = shape(1.0, true, false, 0, 48);
    assert_eq!(hit(&s, s.status.x + 2, 1, 48), Some(Part::Status));
    assert_eq!(hit(&s, s.status.x + 2, 47, 48), Some(Part::Status));
    assert_eq!(hit(&s, s.clock.x + 2, 24, 48), Some(Part::Clock));
    assert_eq!(hit(&s, s.desktop.x + 2, 24, 48), Some(Part::Desktop));
    assert_eq!(hit(&s, s.desktop.x + s.desktop.w + 10, 24, 48), None);
    assert_eq!(hit(&s, s.status.x + 2, 48, 48), None);
}

#[test]
fn the_pill_sits_on_the_dock_edge_at_the_right_and_yields_to_a_wide_dock() {
    let mon = (0, 0, 2560, 1440);
    let s = shape(1.0, false, false, 0, 48);
    let (x, y) = place(mon, 1.0, true, 1500, s.width).unwrap();
    assert_eq!((x, y), (2560 - 6 - s.width, 1440 - 6 - 48));
    assert_eq!(place(mon, 1.0, false, 1500, s.width).unwrap().1, 6);
    assert!(place(mon, 1.0, true, 2500, s.width).is_none());
}

#[test]
fn clock_and_date_follow_the_language() {
    assert_eq!(clock_text(7, 5), "07:05");
    assert_eq!(date_text(19, 11, 2026, Lang::Es), "19/11/2026");
    assert_eq!(date_text(19, 11, 2026, Lang::En), "11/19/2026");
}

#[test]
fn volume_label_shows_percent_or_nothing() {
    assert_eq!(volume_label(Some((40, false))), "40%");
    assert_eq!(volume_label(None), "");
}

#[test]
fn wheel_steps_keep_the_sign_and_never_stall() {
    assert_eq!(wheel_step(120), 2);
    assert_eq!(wheel_step(-120), -2);
    assert_eq!(wheel_step(30), 1);
    assert_eq!(wheel_step(-30), -1);
}

#[test]
fn the_cluster_follows_the_configured_monitor() {
    let primary = (0, 0, 2560, 1440);
    let second = (2560, 0, 4480, 1080);
    assert!(owns("primary", primary, 0) && !owns("primary", second, 1));
    assert!(owns("", primary, 1));
    assert!(owns("2", second, 1) && !owns("2", primary, 0));
    assert!(!owns("off", primary, 0));
    assert!(!owns("nonsense", primary, 0));
}

#[test]
fn a_setting_picks_exactly_one_dock() {
    let docks = [(0, 0, 2560, 1440), (2560, 0, 4480, 1080)];
    let owners = |setting: &str| docks.iter().enumerate().filter(|(i, m)| owns(setting, **m, *i)).map(|(i, _)| i).collect::<Vec<_>>();
    assert_eq!((owners("primary"), owners(""), owners("2"), owners("off"), owners("3")), (vec![0], vec![0], vec![1], vec![], vec![]));
}

#[test]
fn a_crowded_dock_leaves_room_for_the_cluster_on_a_narrow_monitor() {
    for scale in [1.0f32, 1.25, 1.5, 2.0] {
        for w in [1366, 1536, 1920, 2560, 3840].into_iter().filter(|w| *w as f32 / scale >= 1280.0) {
            let mon = (0, 0, w, 1080);
            let bar = crate::dock::layout_for(60, scale, mon, true, 6, true).bar;
            let width = shape(scale, true, true, crate::tray::PINNED_MAX, 48).width;
            assert!(place(mon, scale, true, bar.x + bar.w, width).is_some(), "scale {scale} width {w}: bar right edge {}", bar.x + bar.w);
        }
    }
}

#[test]
fn the_reveal_edge_also_covers_the_pill_at_the_right_of_the_dock() {
    let mon = (0, 0, 1920, 1080);
    assert!(edge_zone(mon, true, 1720, 188, 48, (1800, 1079)));
    assert!(edge_zone(mon, true, 1720, 188, 48, (1680, 1079)));
    assert!(!edge_zone(mon, true, 1720, 188, 48, (1800, 1000)));
    assert!(!edge_zone(mon, true, 1720, 188, 48, (1000, 1079)));
    assert!(edge_zone(mon, false, 1720, 188, 48, (1800, 0)));
}

#[test]
fn pinned_tray_icons_sit_left_of_the_status_without_overlapping_it() {
    let s = shape(1.0, true, true, 3, 48);
    let plain = shape(1.0, true, true, 0, 48);
    assert_eq!(s.tray.len(), 3);
    assert!(s.tray[2].x + s.tray[2].w < s.status.x);
    assert!(s.width > plain.width);
    assert_eq!(hit(&s, s.tray[1].x + 2, 24, 48), Some(Part::Tray(1)));
    assert_eq!(hit(&s, s.status.x + 2, 24, 48), Some(Part::Status));
    assert_eq!(plain.tray.len(), 0);
}

#[test]
fn the_tray_chevron_leads_the_pill_and_opens_before_any_pinned_icon() {
    for pinned in [0, 3] {
        let s = shape(1.0, true, true, pinned, 48);
        assert!(s.chevron.w > 0 && s.chevron.x + s.chevron.w <= s.status.x);
        assert_eq!(hit(&s, s.chevron.x + 2, 24, 48), Some(Part::Chevron));
        if pinned > 0 {
            assert!(s.chevron.x + s.chevron.w <= s.tray[0].x);
        }
    }
}

#[test]
fn the_status_and_clock_menus_offer_what_windows_offers() {
    use crate::i18n::K;
    let volume = context_entries(Part::Status, StatusPart::Volume);
    assert!(volume.iter().any(|(k, e)| *k == K::MenuMute && *e == MenuEntry::Mute));
    assert!(context_entries(Part::Status, StatusPart::Network).iter().any(|(_, e)| *e == MenuEntry::Open("ms-settings:network")));
    assert!(context_entries(Part::Clock, StatusPart::Volume).iter().any(|(_, e)| *e == MenuEntry::Open("ms-settings:dateandtime")));
    assert!(context_entries(Part::Desktop, StatusPart::Volume).is_empty());
    let s = shape(1.0, true, false, 0, 48);
    assert_eq!(status_part(&s, 1.0, s.status.x + 2), StatusPart::Network);
    assert_eq!(status_part(&s, 1.0, s.status.x + s.status.w - 2), StatusPart::Battery);
}
