// Microbenchmarks del camino caliente de la shell. Corren solo a pedido:
//   cargo test --release --offline bench_ -- --ignored --nocapture --test-threads=1
use crate::dock::{fill_round_a, hit_test, layout_for};
use crate::layout::R;
use std::hint::black_box;
use std::time::Instant;

fn median_ns(iters: u32, mut f: impl FnMut()) -> f64 {
    let mut runs: Vec<f64> = (0..15)
        .map(|_| {
            let t = Instant::now();
            for _ in 0..iters {
                f();
            }
            t.elapsed().as_nanos() as f64 / iters as f64
        })
        .collect();
    runs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    runs[runs.len() / 2]
}

#[test]
#[ignore]
fn bench_layout_for_30_apps_on_4k() {
    let ns = median_ns(20_000, || {
        black_box(layout_for(black_box(30), 2.0, (0, 0, 3840, 2160), true, 6, true));
    });
    println!("BENCH layout_for(30 apps, 4K, scale 2.0): {ns:.0} ns");
}

#[test]
#[ignore]
fn bench_hit_test_30_apps() {
    let l = layout_for(30, 1.0, (0, 0, 1920, 1080), true, 6, true);
    let (x, y) = (l.bar.x + l.cells[20].x + 4, l.bar.y + l.cells[20].y + 4);
    let ns = median_ns(200_000, || {
        black_box(hit_test(black_box(&l), black_box(x), black_box(y)));
    });
    println!("BENCH hit_test(30 apps): {ns:.0} ns");
}

#[test]
#[ignore]
fn bench_fill_round_dock_pill() {
    for (label, scale) in [("100%", 1.0f32), ("200%", 2.0)] {
        let l = layout_for(12, scale, (0, 0, 3840, 2160), true, 6, true);
        let (w, h) = (l.bar.w, l.bar.h);
        let mut buf = vec![0u32; (w * h) as usize];
        let ns = median_ns(500, || {
            fill_round_a(black_box(&mut buf), w, h, R { x: 0, y: 0, w, h }, h / 2, (24, 26, 32), 0.82);
        });
        println!("BENCH fill_round_a(dock pill {w}x{h} @{label}): {:.1} us", ns / 1000.0);
    }
}
