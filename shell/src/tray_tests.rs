use super::*;

fn packet(message: u32, hwnd: u32, uid: u32, flags: u32, tip: &str, state: (u32, u32), version: u32, pixels: Option<(u32, u32, Vec<u8>)>) -> Vec<u8> {
    let mut payload = vec![0u8; 964];
    let put = |p: &mut Vec<u8>, off: usize, v: u32| p[off..off + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut payload, 0, 0x34753423);
    put(&mut payload, 4, message);
    put(&mut payload, 8, 964);
    put(&mut payload, 12, hwnd);
    put(&mut payload, 16, uid);
    put(&mut payload, 20, flags);
    put(&mut payload, 24, 0x8001);
    for (i, u) in tip.encode_utf16().enumerate() {
        payload[32 + i * 2..34 + i * 2].copy_from_slice(&u.to_le_bytes());
    }
    put(&mut payload, 288, state.0);
    put(&mut payload, 292, state.1);
    put(&mut payload, 808, version);
    let (w, h, px) = pixels.unwrap_or((0, 0, Vec::new()));
    let mut out = Vec::new();
    for v in [PACKET_ICON] {
        out.extend(v.to_le_bytes());
    }
    out.extend(1u64.to_le_bytes());
    for v in [payload.len() as u32, w, h, px.len() as u32] {
        out.extend(v.to_le_bytes());
    }
    out.extend(payload);
    out.extend(px);
    out
}

fn px(w: u32, h: u32) -> Option<(u32, u32, Vec<u8>)> {
    Some((w, h, vec![7; (w * h * 4) as usize]))
}

fn with_guid(mut frame: Vec<u8>, guid: [u8; 16]) -> Vec<u8> {
    let flags = u32::from_le_bytes(frame[HEADER + 20..HEADER + 24].try_into().unwrap()) | NIF_GUID;
    frame[HEADER + 20..HEADER + 24].copy_from_slice(&flags.to_le_bytes());
    frame[HEADER + 944..HEADER + 960].copy_from_slice(&guid);
    frame
}

#[test]
fn a_hook_frame_parses_into_the_icon_it_announces() {
    let p = parse(&packet(NIM_ADD, 0x1234, 5, NIF_MESSAGE | NIF_ICON | NIF_TIP, "Discord", (0, 0), 0, px(2, 2))).unwrap();
    assert_eq!((p.message, p.hwnd, p.uid, p.callback, p.tip.as_str()), (NIM_ADD, 0x1234, 5, 0x8001, "Discord"));
    assert_eq!(p.icon, px(2, 2));
}

#[test]
fn truncated_or_foreign_frames_are_dropped() {
    let whole = packet(NIM_ADD, 1, 1, NIF_ICON, "", (0, 0), 0, px(2, 2));
    assert!(parse(&whole[..HEADER + 10]).is_none(), "cut inside the notify data");
    let mut log = whole.clone();
    log[0] = 1;
    assert!(parse(&log).is_none());
    assert!(parse(&[]).is_none());
}

#[test]
fn every_prefix_shorter_than_the_header_is_rejected() {
    let whole = packet(NIM_ADD, 1, 1, 0, "", (0, 0), 0, None);
    for len in 0..HEADER {
        assert_eq!(parse(&whole[..len]), None, "prefix length {len}");
    }
}

#[test]
fn a_declared_payload_shorter_than_the_identity_fields_is_rejected() {
    let mut whole = packet(NIM_ADD, 1, 1, 0, "", (0, 0), 0, None);
    for len in [0u32, 12, 20, 31] {
        whole[12..16].copy_from_slice(&len.to_le_bytes());
        assert_eq!(parse(&whole), None, "payload length {len}");
    }
}

#[test]
fn a_payload_length_larger_than_the_frame_is_rejected() {
    let mut whole = packet(NIM_ADD, 1, 1, 0, "", (0, 0), 0, None);
    whole[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(parse(&whole), None);
}

#[test]
fn a_short_legacy_payload_preserves_identity_and_defaults_absent_fields() {
    let mut whole = packet(NIM_ADD, 0x1234, 7, NIF_MESSAGE | NIF_TIP | NIF_STATE, "", (0, 0), 0, None);
    whole[12..16].copy_from_slice(&32u32.to_le_bytes());
    whole.truncate(HEADER + 32);
    assert_eq!(parse(&whole), Some(Packet {
        message: NIM_ADD, hwnd: 0x1234, uid: 7, guid: [0; 16], flags: NIF_MESSAGE | NIF_TIP | NIF_STATE,
        callback: 0x8001, version: 0, tip: String::new(), info: String::new(), info_title: String::new(), info_flags: 0, timeout_ms: 0,
        state: 0, mask: 0, icon: None, balloon_icon: None,
    }));
}

#[test]
fn zero_or_oversized_icon_dimensions_preserve_the_notification_without_pixels() {
    for (w, h) in [(0, 1), (1, 0), (257, 1), (1, 257), (u32::MAX, u32::MAX)] {
        let mut whole = packet(NIM_ADD, 0x1234, 7, NIF_ICON, "", (0, 0), 0, px(1, 1));
        whole[16..20].copy_from_slice(&w.to_le_bytes());
        whole[20..24].copy_from_slice(&h.to_le_bytes());
        let parsed = parse(&whole).unwrap();
        assert_eq!((parsed.hwnd, parsed.uid, parsed.icon), (0x1234, 7, None), "dimensions {w}x{h}");
    }
}

#[test]
fn pixels_that_do_not_match_their_size_are_discarded_but_the_icon_survives() {
    let whole = packet(NIM_ADD, 1, 1, NIF_ICON, "", (0, 0), 0, px(2, 2));
    assert!(parse(&whole[..whole.len() - 1]).unwrap().icon.is_none(), "cut inside the pixels");
    let mut bad = packet(NIM_ADD, 1, 1, NIF_ICON, "", (0, 0), 0, Some((2, 2, vec![1; 4])));
    bad[24..28].copy_from_slice(&4u32.to_le_bytes());
    assert!(parse(&bad).unwrap().icon.is_none(), "4 bytes cannot be a 2x2 icon");
}

fn feed(icons: &mut Vec<Icon>, frame: Vec<u8>) -> bool {
    apply(icons, parse(&frame).unwrap())
}

#[test]
fn add_modify_hide_and_delete_keep_the_list_in_step() {
    let mut icons = Vec::new();
    assert!(feed(&mut icons, packet(NIM_ADD, 1, 1, NIF_MESSAGE | NIF_ICON | NIF_TIP, "A", (0, 0), 0, px(2, 2))));
    assert!(feed(&mut icons, packet(NIM_ADD, 2, 1, NIF_MESSAGE | NIF_ICON, "", (0, 0), 0, px(2, 2))));
    assert_eq!(icons.len(), 2);
    assert!(!feed(&mut icons, packet(NIM_MODIFY, 1, 1, NIF_TIP, "A", (0, 0), 0, None)), "same tip is not a change");
    assert!(feed(&mut icons, packet(NIM_MODIFY, 1, 1, NIF_TIP, "B", (0, 0), 0, None)));
    assert_eq!(icons[0].tip, "B");
    assert_eq!(icons[0].bgra.len(), 16, "an update without pixels keeps the previous icon");
    assert!(feed(&mut icons, packet(NIM_MODIFY, 1, 1, NIF_STATE, "", (NIS_HIDDEN, NIS_HIDDEN), 0, None)));
    assert!(icons[0].hidden);
    assert!(feed(&mut icons, packet(NIM_MODIFY, 1, 1, NIF_STATE, "", (0, NIS_HIDDEN), 0, None)));
    assert!(!icons[0].hidden);
    assert!(feed(&mut icons, packet(NIM_DELETE, 1, 1, 0, "", (0, 0), 0, None)));
    assert_eq!(icons.iter().map(|i| i.hwnd).collect::<Vec<_>>(), vec![2]);
    assert!(!feed(&mut icons, packet(NIM_DELETE, 9, 9, 0, "", (0, 0), 0, None)));
}

#[test]
fn the_version_only_counts_in_set_version_because_modify_shares_it_with_the_timeout() {
    let mut icons = Vec::new();
    feed(&mut icons, packet(NIM_ADD, 1, 1, NIF_MESSAGE, "", (0, 0), 0, None));
    feed(&mut icons, packet(NIM_SETVERSION, 1, 1, 0, "", (0, 0), 4, None));
    assert_eq!(icons[0].version, 4);
    feed(&mut icons, packet(NIM_MODIFY, 1, 1, NIF_TIP, "x", (0, 0), 9000, None));
    assert_eq!(icons[0].version, 4);
}

#[test]
fn adding_the_same_identity_updates_in_place_without_duplicating_it() {
    let mut icons = Vec::new();
    feed(&mut icons, packet(NIM_ADD, 1, 7, NIF_TIP, "before", (0, 0), 0, None));
    feed(&mut icons, packet(NIM_ADD, 2, 7, NIF_TIP, "neighbor", (0, 0), 0, None));
    assert!(feed(&mut icons, packet(NIM_ADD, 1, 7, NIF_TIP, "after", (0, 0), 0, None)));
    assert_eq!(icons.iter().map(|i| (i.hwnd, i.uid, i.tip.as_str())).collect::<Vec<_>>(), vec![(1, 7, "after"), (2, 7, "neighbor")]);
}

#[test]
fn guid_icons_use_guid_identity_even_when_window_and_uid_match() {
    let mut icons = Vec::new();
    let a = [1; 16];
    let b = [2; 16];
    let base = || packet(NIM_ADD, 1, 7, NIF_TIP, "icon", (0, 0), 0, None);
    feed(&mut icons, with_guid(base(), a));
    feed(&mut icons, with_guid(base(), b));
    assert_eq!(icons.len(), 2);
    assert!(feed(&mut icons, with_guid(packet(NIM_MODIFY, 1, 9, NIF_TIP, "updated", (0, 0), 0, None), a)));
    assert_eq!(icons.iter().map(|i| (i.guid, i.uid, i.tip.as_str())).collect::<Vec<_>>(), vec![(a, 9, "updated"), (b, 7, "icon")]);
    assert_eq!(key(&icons[0]), format!("guid:{}", "01".repeat(16)));
}

#[test]
fn native_shell_notify_parser_checks_magic_size_and_guid() {
    let frame = with_guid(packet(NIM_ADD, 0x1234, 7, NIF_MESSAGE | NIF_TIP, "Native", (0, 0), 0, None), [9; 16]);
    let mut raw = frame[HEADER..].to_vec();
    assert_eq!(parse_shell_notify(&raw, None, None).unwrap().guid, [9; 16]);
    assert_eq!(parse_shell_notify(&raw[..963], None, None), None);
    raw[0] ^= 1;
    assert_eq!(parse_shell_notify(&raw, None, None), None);
}

#[test]
fn modifying_an_unknown_identity_does_not_create_an_icon() {
    let mut icons = Vec::new();
    assert!(!feed(&mut icons, packet(NIM_MODIFY, 1, 7, NIF_TIP, "unknown", (0, 0), 0, None)));
    assert_eq!(icons, Vec::<Icon>::new());
    feed(&mut icons, packet(NIM_ADD, 1, 8, NIF_TIP, "existing", (0, 0), 0, None));
    assert!(!feed(&mut icons, packet(NIM_MODIFY, 1, 7, NIF_TIP, "unknown", (0, 0), 0, None)));
    assert_eq!(icons.iter().map(|i| (i.hwnd, i.uid, i.tip.as_str())).collect::<Vec<_>>(), vec![(1, 8, "existing")]);
}

#[test]
fn setting_version_for_an_unknown_identity_does_not_create_an_icon() {
    let mut icons = Vec::new();
    assert!(!feed(&mut icons, packet(NIM_SETVERSION, 1, 7, 0, "", (0, 0), 4, None)));
    assert_eq!(icons, Vec::<Icon>::new());
    feed(&mut icons, packet(NIM_ADD, 2, 7, 0, "", (0, 0), 0, None));
    assert!(!feed(&mut icons, packet(NIM_SETVERSION, 1, 7, 0, "", (0, 0), 4, None)));
    assert_eq!(icons.iter().map(|i| (i.hwnd, i.uid, i.version)).collect::<Vec<_>>(), vec![(2, 7, 0)]);
}

#[test]
fn deleting_by_guid_keeps_other_guid_and_legacy_icons_with_the_same_window_and_id() {
    let mut icons = Vec::new();
    let base = || packet(NIM_ADD, 1, 7, NIF_TIP, "icon", (0, 0), 0, None);
    feed(&mut icons, base());
    feed(&mut icons, with_guid(base(), [1; 16]));
    feed(&mut icons, with_guid(base(), [2; 16]));
    assert!(!feed(&mut icons, with_guid(packet(NIM_DELETE, 1, 7, 0, "", (0, 0), 0, None), [3; 16])));
    assert_eq!(icons.iter().map(|i| i.guid).collect::<Vec<_>>(), vec![[0; 16], [1; 16], [2; 16]]);
    assert!(feed(&mut icons, with_guid(packet(NIM_DELETE, 9, 9, 0, "", (0, 0), 0, None), [1; 16])));
    assert_eq!(icons.iter().map(|i| (i.hwnd, i.uid, i.guid)).collect::<Vec<_>>(), vec![(1, 7, [0; 16]), (1, 7, [2; 16])]);
}

#[test]
fn modifying_an_icon_requires_both_its_window_and_id_to_match() {
    let mut icons = Vec::new();
    for (hwnd, uid) in [(1, 7), (1, 8), (2, 7)] {
        feed(&mut icons, packet(NIM_ADD, hwnd, uid, NIF_TIP, "original", (0, 0), 0, None));
    }
    assert!(feed(&mut icons, packet(NIM_MODIFY, 1, 7, NIF_TIP, "updated", (0, 0), 0, None)));
    assert_eq!(icons.iter().map(|i| (i.hwnd, i.uid, i.tip.as_str())).collect::<Vec<_>>(), vec![(1, 7, "updated"), (1, 8, "original"), (2, 7, "original")]);
}

#[test]
fn deleting_an_icon_keeps_neighbors_sharing_its_window_or_id() {
    let mut icons = Vec::new();
    for (hwnd, uid) in [(1, 7), (1, 8), (2, 7)] {
        feed(&mut icons, packet(NIM_ADD, hwnd, uid, 0, "", (0, 0), 0, None));
    }
    assert!(feed(&mut icons, packet(NIM_DELETE, 1, 7, 0, "", (0, 0), 0, None)));
    assert_eq!(icons.iter().map(|i| (i.hwnd, i.uid)).collect::<Vec<_>>(), vec![(1, 8), (2, 7)]);
}

#[test]
fn setting_version_updates_only_the_matching_identity_and_preserves_its_contents() {
    let mut icons = Vec::new();
    for (hwnd, uid) in [(1, 7), (1, 8), (2, 7)] {
        feed(&mut icons, packet(NIM_ADD, hwnd, uid, NIF_MESSAGE | NIF_TIP | NIF_ICON, "original", (0, 0), 0, px(1, 1)));
    }
    assert!(feed(&mut icons, packet(NIM_SETVERSION, 1, 7, NIF_TIP | NIF_ICON | NIF_STATE, "ignored", (NIS_HIDDEN, NIS_HIDDEN), 4, px(2, 2))));
    assert_eq!(icons.iter().map(|i| (i.hwnd, i.uid, i.version)).collect::<Vec<_>>(), vec![(1, 7, 4), (1, 8, 0), (2, 7, 0)]);
    assert_eq!((icons[0].callback, icons[0].tip.as_str(), icons[0].hidden, icons[0].size, icons[0].bgra.as_slice()), (0x8001, "original", false, (1, 1), &[7u8; 4][..]));
    assert!(!feed(&mut icons, packet(NIM_SETVERSION, 1, 7, 0, "", (0, 0), 4, None)));
}

#[test]
fn icons_without_pixels_or_hidden_ones_are_not_shown() {
    let mut all = ICONS.lock().unwrap();
    let keep = std::mem::take(&mut *all);
    all.push(Icon { hwnd: 1, uid: 1, guid: [0; 16], callback: 0, version: 0, tip: String::new(), hidden: false, exe: String::new(), size: (1, 1), bgra: vec![0; 4] });
    all.push(Icon { hwnd: 2, uid: 1, guid: [0; 16], callback: 0, version: 0, tip: String::new(), hidden: true, exe: String::new(), size: (1, 1), bgra: vec![0; 4] });
    all.push(Icon { hwnd: 3, uid: 1, guid: [0; 16], callback: 0, version: 0, tip: String::new(), hidden: false, exe: String::new(), size: (0, 0), bgra: Vec::new() });
    drop(all);
    assert_eq!(with_visible(|v| v.iter().map(|i| i.hwnd).collect::<Vec<_>>()), vec![1]);
    *ICONS.lock().unwrap() = keep;
}

#[test]
fn gestures_send_what_explorer_sends() {
    assert_eq!(messages_for(Gesture::Left, 0), vec![WM_LBUTTONDOWN, WM_LBUTTONUP]);
    assert_eq!(messages_for(Gesture::Left, 4), vec![WM_LBUTTONDOWN, WM_LBUTTONUP, NIN_SELECT]);
    assert_eq!(messages_for(Gesture::LeftDouble, 4), vec![WM_LBUTTONDBLCLK, WM_LBUTTONUP]);
    assert_eq!(messages_for(Gesture::Right, 4), vec![WM_RBUTTONDOWN, WM_RBUTTONUP, WM_CONTEXTMENU]);
    assert_eq!(messages_for(Gesture::Right, 1), vec![WM_RBUTTONDOWN, WM_RBUTTONUP]);
    assert_eq!(messages_for(Gesture::Middle, 4), vec![WM_MBUTTONDOWN, WM_MBUTTONUP]);
}

#[test]
fn callback_parameters_follow_the_icon_version() {
    assert_eq!(callback_params(4, 7, WM_LBUTTONUP, (100, 200)), ((200 << 16) | 100, ((7 << 16) | WM_LBUTTONUP) as isize));
    assert_eq!(callback_params(0, 7, WM_LBUTTONUP, (100, 200)), (7, WM_LBUTTONUP as isize));
    assert_eq!(callback_params(4, 7, WM_LBUTTONUP, (-5, 10)).0 & 0xFFFF, 0xFFFB);
}

#[test]
fn stale_window_cleanup_removes_only_dead_owners_and_reports_changes() {
    let mut icons = vec![icon("alive.exe", 1, false, true), icon("dead.exe", 2, false, true)];
    icons[1].hwnd = 2;
    assert!(super::retain_live_icons(&mut icons, |hwnd| hwnd == 1));
    assert_eq!(icons.iter().map(|icon| icon.hwnd).collect::<Vec<_>>(), [1]);
    assert!(!super::retain_live_icons(&mut icons, |_| true));
}

fn icon(exe: &str, uid: u32, hidden: bool, with_pixels: bool) -> Icon {
    Icon { hwnd: 1, uid, guid: [0; 16], callback: 0, version: 0, tip: String::new(), hidden, exe: exe.into(), size: (1, 1), bgra: if with_pixels { vec![0; 4] } else { Vec::new() } }
}

#[test]
fn a_pinned_icon_is_identified_by_its_exe_and_id() {
    assert_eq!(key(&icon("drive.exe", 7, false, true)), "drive.exe:7");
}

#[test]
fn pinned_icons_follow_pin_order_and_skip_missing_hidden_or_blank_ones() {
    let all = vec![icon("a.exe", 1, false, true), icon("b.exe", 1, false, true), icon("c.exe", 1, true, true), icon("d.exe", 1, false, false)];
    let pins: Vec<String> = ["b.exe:1", "gone.exe:1", "c.exe:1", "d.exe:1", "a.exe:1"].iter().map(|s| s.to_string()).collect();
    let got: Vec<String> = pinned_of(&all, &pins).iter().map(|i| key(i)).collect();
    assert_eq!(got, ["b.exe:1", "a.exe:1"]);
}

#[test]
fn only_the_first_pinned_icons_up_to_the_limit_show() {
    let all: Vec<Icon> = (0..PINNED_MAX as u32 + 2).map(|i| icon("x.exe", i, false, true)).collect();
    let pins: Vec<String> = all.iter().map(key).collect();
    assert_eq!(pinned_of(&all, &pins).len(), PINNED_MAX);
}

#[test]
fn a_balloon_frame_preserves_utf16_text_title_flags_and_timeout() {
    let mut frame = packet(NIM_MODIFY, 10, 7, NIF_INFO, "", (0, 0), 9000, None);
    for (offset, text) in [(296, "Copia lista 🚀"), (812, "Archivos 日本語")] {
        for (i, unit) in text.encode_utf16().enumerate() {
            let start = HEADER + offset + i * 2;
            frame[start..start + 2].copy_from_slice(&unit.to_le_bytes());
        }
    }
    frame[HEADER + 940..HEADER + 944].copy_from_slice(&0x21u32.to_le_bytes());
    let parsed = parse(&frame).unwrap();
    assert_eq!((parsed.info.as_str(), parsed.info_title.as_str(), parsed.info_flags, parsed.timeout_ms), ("Copia lista 🚀", "Archivos 日本語", 0x21, 9000));
}

#[test]
fn a_custom_balloon_icon_is_kept_separate_from_the_tray_icon() {
    let frame = packet(NIM_MODIFY, 10, 7, NIF_INFO, "", (0, 0), 9000, None);
    let mut payload = frame[HEADER..].to_vec();
    payload[940..944].copy_from_slice(&4u32.to_le_bytes());
    let parsed = parse_shell_notify(&payload, Some((1, 1, vec![1, 2, 3, 4])), Some((1, 1, vec![5, 6, 7, 8]))).unwrap();
    assert_eq!(parsed.icon, Some((1, 1, vec![1, 2, 3, 4])));
    assert_eq!(parsed.balloon_icon, Some((1, 1, vec![5, 6, 7, 8])));
}

fn balloon(sequence: u64, hwnd: u32, uid: u32, text: &str) -> BalloonRequest {
    BalloonRequest { sequence, hwnd, uid, guid: [0; 16], callback: 0x8001, version: 4, title: "Transferencia".into(), text: text.into(), flags: 1, realtime: false, timeout_ms: 9000, icon: None }
}

#[test]
fn the_first_balloon_is_presented_immediately() {
    let mut queue = BalloonQueue::default();
    assert!(queue.submit(balloon(1, 10, 1, "Primera")));
    assert_eq!(queue.current().map(|r| (r.hwnd, r.uid, r.title.as_str(), r.text.as_str(), r.flags, r.timeout_ms, r.version)), Some((10, 1, "Transferencia", "Primera", 1, 9000, 4)));
    assert_eq!(queue.pending_len(), 0);
}

#[test]
fn repeated_identical_balloons_are_queued_individually() {
    let mut queue = BalloonQueue::default();
    queue.submit(balloon(2, 10, 1, "Repetida"));
    assert!(queue.submit(balloon(3, 10, 1, "Repetida")));
    assert_eq!(queue.pending_len(), 1);
    queue.finish();
    assert_eq!(queue.current().map(|r| r.text.as_str()), Some("Repetida"));
    queue.finish();
    assert!(queue.current().is_none());
}

#[test]
fn realtime_balloons_are_discarded_when_another_is_visible() {
    let mut queue = BalloonQueue::default();
    queue.submit(balloon(4, 10, 1, "Actual"));
    let mut realtime = balloon(5, 20, 2, "Urgente");
    realtime.realtime = true;
    assert!(!queue.submit(realtime));
    assert_eq!(queue.current().map(|r| r.text.as_str()), Some("Actual"));
    assert_eq!(queue.pending_len(), 0);
}

#[test]
fn realtime_requests_are_dropped_but_ordinary_requests_wait_for_the_active_balloon() {
    let mut queue = BalloonQueue::default();
    assert!(queue.submit(balloon(6, 10, 1, "Active")));
    let mut realtime = balloon(7, 20, 2, "Realtime");
    realtime.realtime = true;
    assert_eq!(queue.submit(realtime), false);
    let mut ordinary = balloon(8, 30, 3, "Ordinary");
    ordinary.flags = 0x40;
    assert_eq!(queue.submit(ordinary), true);
    assert_eq!(queue.current().map(|r| r.text.as_str()), Some("Active"));
    assert_eq!(queue.finish().map(|r| r.text), Some("Ordinary".into()));
    assert_eq!(queue.finish(), None);
}

#[test]
fn realtime_balloons_are_presented_when_idle() {
    let mut queue = BalloonQueue::default();
    let mut realtime = balloon(9, 10, 1, "Urgente");
    realtime.realtime = true;
    assert!(queue.submit(realtime));
    assert_eq!(queue.current().map(|r| r.text.as_str()), Some("Urgente"));
}

#[test]
fn finishing_balloons_advances_in_submission_order() {
    let mut queue = BalloonQueue::default();
    for (uid, text) in [(1, "Primera"), (2, "Segunda"), (3, "Tercera")] {
        queue.submit(balloon(uid as u64, 10, uid, text));
    }
    assert_eq!(queue.finish().map(|r| r.text), Some("Segunda".into()));
    assert_eq!(queue.current().map(|r| r.text.as_str()), Some("Segunda"));
    assert_eq!(queue.finish().map(|r| r.text), Some("Tercera".into()));
    assert_eq!(queue.current().map(|r| r.text.as_str()), Some("Tercera"));
    queue.finish();
    assert!(queue.current().is_none());
    assert_eq!(queue.pending_len(), 0);
    assert!(queue.finish().is_none());
}

#[test]
fn cancellation_removes_the_active_balloon_and_all_queued_copies() {
    let mut queue = BalloonQueue::default();
    queue.submit(balloon(11, 10, 1, "Actual"));
    queue.submit(balloon(12, 20, 1, "Otra ventana"));
    queue.submit(balloon(13, 10, 1, "Pendiente"));
    queue.submit(balloon(14, 10, 2, "Otro id"));
    let (removed, next) = queue.cancel(10, 1, [0; 16]);
    assert_eq!(removed.map(|r| r.text), Some("Actual".into()));
    assert_eq!(next.map(|r| r.text), Some("Otra ventana".into()));
    assert_eq!(queue.current().map(|r| r.text.as_str()), Some("Otra ventana"));
    assert_eq!(queue.pending_len(), 1);
    queue.finish();
    assert_eq!(queue.current().map(|r| r.text.as_str()), Some("Otro id"));
}

#[test]
fn cancellation_of_a_queued_identity_leaves_the_active_balloon_alone() {
    let mut queue = BalloonQueue::default();
    queue.submit(balloon(15, 10, 1, "Actual"));
    queue.submit(balloon(16, 20, 2, "Pendiente"));
    let (removed, next) = queue.cancel(20, 2, [0; 16]);
    assert!(removed.is_none());
    assert!(next.is_none());
    assert_eq!(queue.current().map(|r| r.text.as_str()), Some("Actual"));
    assert_eq!(queue.pending_len(), 0);
}

#[test]
fn guid_identity_ignores_the_window_and_numeric_id() {
    let mut queue = BalloonQueue::default();
    let mut first = balloon(17, 10, 1, "Actual");
    first.guid = [7; 16];
    queue.submit(first);
    let mut next = balloon(18, 20, 2, "Pendiente");
    next.guid = [7; 16];
    queue.submit(next);
    let (removed, next) = queue.cancel(10, 1, [8; 16]);
    assert!(removed.is_none());
    assert!(next.is_none());
    assert_eq!(queue.current().map(|r| r.text.as_str()), Some("Actual"));
    assert_eq!(queue.cancel(99, 99, [7; 16]).0.map(|r| r.text), Some("Actual".into()));
    assert!(queue.current().is_none());
    assert_eq!(queue.pending_len(), 0);
}

#[test]
fn an_empty_info_notification_cancels_its_identity() {
    let parsed = parse(&packet(NIM_MODIFY, 10, 1, NIF_INFO, "", (0, 0), 0, None)).unwrap();
    assert_eq!(parsed.info, "");
    let mut queue = BalloonQueue::default();
    queue.submit(balloon(19, 10, 1, "Actual"));
    queue.submit(balloon(20, 10, 1, "Pendiente"));
    assert_eq!(queue.cancel(parsed.hwnd, parsed.uid, [0; 16]).0.map(|r| r.text), Some("Actual".into()));
    assert!(queue.current().is_none());
    assert_eq!(queue.pending_len(), 0);
}

#[test]
fn a_stale_completion_keeps_the_balloon_presented_after_cancellation() {
    let mut queue = BalloonQueue::default();
    let first = balloon(1, 10, 1, "First");
    queue.submit(first.clone());
    queue.submit(balloon(2, 20, 2, "Second"));
    queue.submit(balloon(3, 30, 3, "Third"));
    queue.cancel(10, 1, [0; 16]);

    assert_eq!(queue.finish_if(&first), None);
    assert_eq!(queue.current().map(|r| (r.sequence, r.hwnd, r.uid, r.text.as_str())), Some((2, 20, 2, "Second")));
    assert_eq!(queue.pending_len(), 1);
}

#[test]
fn callback_updates_are_delivered_when_the_current_balloon_finishes() {
    let mut queue = BalloonQueue::default();
    let current = balloon(1, 10, 1, "Current");
    queue.submit(current.clone());
    queue.update_callback(10, 1, [0; 16], Some(0x8002), None);
    queue.update_callback(10, 1, [0; 16], None, Some(0));

    assert_eq!(queue.finish_if(&current).map(|(r, _)| (r.hwnd, r.uid, r.callback, r.version)), Some((10, 1, 0x8002, 0)));
}

#[test]
fn callback_updates_follow_matching_pending_balloons_without_changing_neighbors() {
    let mut queue = BalloonQueue::default();
    queue.submit(balloon(1, 20, 2, "Current"));
    queue.submit(balloon(2, 10, 1, "Pending"));
    queue.submit(balloon(3, 10, 2, "Other id"));
    queue.submit(balloon(4, 20, 1, "Other window"));
    queue.update_callback(10, 1, [0; 16], Some(0x8002), None);
    queue.update_callback(10, 1, [0; 16], None, Some(0));

    assert_eq!(queue.current().map(|r| (r.callback, r.version)), Some((0x8001, 4)));
    assert_eq!(queue.finish().map(|r| (r.hwnd, r.uid, r.callback, r.version)), Some((10, 1, 0x8002, 0)));
    assert_eq!(queue.finish().map(|r| (r.hwnd, r.uid, r.callback, r.version)), Some((10, 2, 0x8001, 4)));
    assert_eq!(queue.finish().map(|r| (r.hwnd, r.uid, r.callback, r.version)), Some((20, 1, 0x8001, 4)));
}

#[test]
fn only_add_and_modify_with_info_carry_balloons() {
    for (message, flags, expected) in [
        (NIM_ADD, NIF_INFO, true),
        (NIM_MODIFY, NIF_INFO | NIF_MESSAGE, true),
        (NIM_ADD, NIF_MESSAGE, false),
        (NIM_MODIFY, 0, false),
        (NIM_SETVERSION, NIF_INFO | NIF_MESSAGE, false),
        (NIM_DELETE, NIF_INFO, false),
    ] {
        assert_eq!(carries_balloon(message, flags), expected, "message {message}, flags {flags}");
    }
}
