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
fn icons_without_pixels_or_hidden_ones_are_not_shown() {
    let mut all = ICONS.lock().unwrap();
    let keep = std::mem::take(&mut *all);
    all.push(Icon { hwnd: 1, uid: 1, callback: 0, version: 0, tip: String::new(), hidden: false, exe: String::new(), size: (1, 1), bgra: vec![0; 4] });
    all.push(Icon { hwnd: 2, uid: 1, callback: 0, version: 0, tip: String::new(), hidden: true, exe: String::new(), size: (1, 1), bgra: vec![0; 4] });
    all.push(Icon { hwnd: 3, uid: 1, callback: 0, version: 0, tip: String::new(), hidden: false, exe: String::new(), size: (0, 0), bgra: Vec::new() });
    drop(all);
    assert_eq!(with_visible(|v| v.iter().map(|i| i.hwnd).collect::<Vec<_>>()), vec![1]);
    *ICONS.lock().unwrap() = keep;
}

#[test]
fn gestures_send_what_explorer_sends() {
    assert_eq!(messages_for(Gesture::Left, 0), vec![WM_LBUTTONDOWN, WM_LBUTTONUP]);
    assert_eq!(messages_for(Gesture::Left, 4), vec![WM_LBUTTONDOWN, WM_LBUTTONUP, NIN_SELECT]);
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

fn icon(exe: &str, uid: u32, hidden: bool, with_pixels: bool) -> Icon {
    Icon { hwnd: 1, uid, callback: 0, version: 0, tip: String::new(), hidden, exe: exe.into(), size: (1, 1), bgra: if with_pixels { vec![0; 4] } else { Vec::new() } }
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
