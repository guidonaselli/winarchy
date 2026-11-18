use super::*;

#[test]
fn zero_radius_rectangle_fully_covers_its_pixels() {
    let mut buf = vec![0; 4 * 4];
    fill_round_a(&mut buf, 4, 4, R { x: 1, y: 1, w: 2, h: 2 }, 0, (32, 48, 64), 1.0);
    assert_eq!(buf, [
        0, 0, 0, 0,
        0, 0xff203040, 0xff203040, 0,
        0, 0xff203040, 0xff203040, 0,
        0, 0, 0, 0,
    ]);
}

#[test]
fn attention_marker_keeps_its_urgent_center_and_contrasting_halo_opaque() {
    for (urgent, center, halo) in [
        ((247, 118, 142), 0xfff7768e, 0xff000000),
        ((86, 53, 66), 0xff563542, 0xffffffff),
    ] {
        for (opacity, background) in [(1.0, 0xff203040), (0.25, 0x3f080c10)] {
            let mut buf = vec![0; 16 * 16];
            fill_round_a(&mut buf, 16, 16, R { x: 0, y: 0, w: 16, h: 16 }, 0, (32, 48, 64), opacity);
            draw_attention_marker(&mut buf, 16, 16, R { x: 5, y: 5, w: 6, h: 6 }, 2, urgent);
            assert_eq!((buf[8 * 16 + 8], buf[8 * 16 + 4], buf[0]), (center, halo, background), "urgent={urgent:?}, opacity={opacity}");
        }
    }
}
