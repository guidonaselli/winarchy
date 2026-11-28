use super::*;

#[test]
fn stretch_uses_the_full_source_at_the_virtual_desktop_offset() {
    let layout = frame_layout(
        WallpaperPosition::Stretch,
        4000,
        2000,
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        RECT {
            left: -1280,
            top: -120,
            right: 1920,
            bottom: 1080,
        },
    )
    .unwrap();
    assert_eq!(
        (
            layout.source.x,
            layout.source.y,
            layout.source.width,
            layout.source.height,
            layout.destination.left,
            layout.destination.top,
            layout.destination.right,
            layout.destination.bottom
        ),
        (0, 0, 4000, 2000, 1280, 120, 3200, 1200)
    );
}

#[test]
fn center_preserves_native_pixels_with_background_margins() {
    let layout = frame_layout(
        WallpaperPosition::Center,
        800,
        600,
        RECT {
            left: -1920,
            top: -120,
            right: 0,
            bottom: 960,
        },
        RECT {
            left: -1920,
            top: -120,
            right: 2560,
            bottom: 1440,
        },
    )
    .unwrap();
    assert_eq!(
        (
            layout.source.x,
            layout.source.y,
            layout.source.width,
            layout.source.height,
            layout.destination.left,
            layout.destination.top,
            layout.destination.right,
            layout.destination.bottom
        ),
        (0, 0, 800, 600, 560, 240, 1360, 840)
    );
}

#[test]
fn center_crops_an_oversized_image_without_scaling() {
    let layout = frame_layout(
        WallpaperPosition::Center,
        4000,
        2000,
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
    )
    .unwrap();
    assert_eq!(
        (
            layout.source.x,
            layout.source.y,
            layout.source.width,
            layout.source.height,
            layout.destination.left,
            layout.destination.top,
            layout.destination.right,
            layout.destination.bottom
        ),
        (1040, 460, 1920, 1080, 0, 0, 1920, 1080)
    );
}

#[test]
fn center_crops_only_the_oversized_axis() {
    let layout = frame_layout(
        WallpaperPosition::Center,
        2000,
        600,
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
    )
    .unwrap();
    assert_eq!(
        (
            layout.source.x,
            layout.source.y,
            layout.source.width,
            layout.source.height,
            layout.destination.left,
            layout.destination.top,
            layout.destination.right,
            layout.destination.bottom
        ),
        (40, 0, 1920, 600, 0, 240, 1920, 840)
    );
}

#[test]
fn fit_preserves_a_wide_source_with_top_and_bottom_margins() {
    let layout = frame_layout(
        WallpaperPosition::Fit,
        4000,
        2000,
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        RECT {
            left: -1280,
            top: -120,
            right: 1920,
            bottom: 1080,
        },
    )
    .unwrap();
    assert_eq!(
        (
            layout.source.x,
            layout.source.y,
            layout.source.width,
            layout.source.height,
            layout.destination.left,
            layout.destination.top,
            layout.destination.right,
            layout.destination.bottom
        ),
        (0, 0, 4000, 2000, 1280, 180, 3200, 1140)
    );
}

#[test]
fn fit_preserves_a_tall_source_with_left_and_right_margins() {
    let layout = frame_layout(
        WallpaperPosition::Fit,
        1000,
        2000,
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
    )
    .unwrap();
    assert_eq!(
        (
            layout.source.x,
            layout.source.y,
            layout.source.width,
            layout.source.height,
            layout.destination.left,
            layout.destination.top,
            layout.destination.right,
            layout.destination.bottom
        ),
        (0, 0, 1000, 2000, 690, 0, 1230, 1080)
    );
}

#[test]
fn fill_crops_the_source_and_covers_the_monitor() {
    let layout = frame_layout(
        WallpaperPosition::Fill,
        4000,
        2000,
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        RECT {
            left: -1280,
            top: -120,
            right: 1920,
            bottom: 1080,
        },
    )
    .unwrap();
    assert_eq!(
        (
            layout.source.x,
            layout.source.y,
            layout.source.width,
            layout.source.height,
            layout.destination.left,
            layout.destination.top,
            layout.destination.right,
            layout.destination.bottom
        ),
        (222, 0, 3555, 2000, 1280, 120, 3200, 1200)
    );
}

#[test]
fn span_fills_the_single_virtual_desktop_frame() {
    let desktop = RECT {
        left: -1920,
        top: -120,
        right: 1920,
        bottom: 1080,
    };
    let layout = frame_layout(WallpaperPosition::Span, 4000, 2000, desktop, desktop).unwrap();
    assert_eq!(
        (
            layout.source.x,
            layout.source.y,
            layout.source.width,
            layout.source.height,
            layout.destination.left,
            layout.destination.top,
            layout.destination.right,
            layout.destination.bottom
        ),
        (0, 375, 4000, 1250, 0, 0, 3840, 1200)
    );
}

#[test]
fn tile_brush_repeats_bgra_pixels_in_both_axes() {
    struct Surface {
        dc: HDC,
        bitmap: HBITMAP,
        previous: Option<HGDIOBJ>,
        brush: HBRUSH,
    }

    impl Drop for Surface {
        fn drop(&mut self) {
            unsafe {
                if let Some(previous) = self.previous {
                    SelectObject(self.dc, previous);
                }
                let _ = DeleteObject(self.bitmap);
                let _ = DeleteObject(self.brush);
                let _ = DeleteDC(self.dc);
            }
        }
    }

    unsafe {
        let mut surface = Surface {
            dc: CreateCompatibleDC(None),
            bitmap: HBITMAP::default(),
            previous: None,
            brush: HBRUSH::default(),
        };
        assert!(!surface.dc.0.is_null());
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: 4,
                biHeight: -4,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        surface.bitmap =
            CreateDIBSection(surface.dc, &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let previous = SelectObject(surface.dc, surface.bitmap);
        assert!(!previous.0.is_null() && previous.0 as isize != -1);
        surface.previous = Some(previous);
        surface.brush = HBRUSH(
            create_tile_brush(
                2,
                2,
                &[
                    0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255,
                ],
            )
            .unwrap() as *mut _,
        );
        let rect = RECT {
            left: 0,
            top: 0,
            right: 4,
            bottom: 4,
        };
        assert_ne!(FillRect(surface.dc, &rect, surface.brush), 0);
        let pixels: Vec<u32> = (0..4)
            .flat_map(|y| (0..4).map(move |x| GetPixel(surface.dc, x, y).0))
            .collect();
        assert_eq!(
            pixels,
            [
                0x0000ff, 0x00ff00, 0x0000ff, 0x00ff00, 0xff0000, 0xffffff, 0xff0000, 0xffffff,
                0x0000ff, 0x00ff00, 0x0000ff, 0x00ff00, 0xff0000, 0xffffff, 0xff0000, 0xffffff,
            ]
        );
    }
}

#[test]
fn tile_leaves_image_layout_to_the_pattern_brush() {
    let desktop = RECT {
        left: -1920,
        top: -120,
        right: 1920,
        bottom: 1080,
    };
    assert!(frame_layout(WallpaperPosition::Tile, 800, 600, desktop, desktop).is_none());
}

#[test]
fn fill_crops_a_wide_image_equally_on_both_sides() {
    let crop = source_crop_fill(4000, 2000, 1920, 1080).unwrap();
    assert_eq!(
        (crop.x, crop.y, crop.width, crop.height),
        (222, 0, 3555, 2000)
    );
}

#[test]
fn fill_crops_a_tall_image_equally_at_top_and_bottom() {
    let crop = source_crop_fill(2000, 4000, 1920, 1080).unwrap();
    assert_eq!(
        (crop.x, crop.y, crop.width, crop.height),
        (0, 1437, 2000, 1125)
    );
}

#[test]
fn fill_preserves_the_whole_image_when_aspect_ratios_match() {
    let crop = source_crop_fill(3840, 2160, 1920, 1080).unwrap();
    assert_eq!(
        (crop.x, crop.y, crop.width, crop.height),
        (0, 0, 3840, 2160)
    );
}

#[test]
fn fill_rejects_a_zero_source_or_destination_dimension() {
    for dimensions in [
        (0, 100, 100, 100),
        (100, 0, 100, 100),
        (100, 100, 0, 100),
        (100, 100, 100, 0),
    ] {
        assert!(source_crop_fill(dimensions.0, dimensions.1, dimensions.2, dimensions.3).is_none());
    }
}

#[test]
fn image_size_is_the_bgra_buffer_length() {
    assert_eq!(valid_image_size(2, 3), Some(24));
}

#[test]
fn image_size_rejects_empty_images() {
    assert_eq!(valid_image_size(0, 10), None);
    assert_eq!(valid_image_size(10, 0), None);
    assert_eq!(valid_image_size(0, 0), None);
}

#[test]
fn image_size_rejects_allocation_overflow() {
    assert_eq!(valid_image_size(u32::MAX, u32::MAX), None);
}

#[test]
fn source_images_are_scaled_to_the_output_budget() {
    let (width, height) = scaled_source_size(6000, 4000, MAX_IMAGE_PIXELS).unwrap();
    assert!(u64::from(width) * u64::from(height) <= MAX_IMAGE_PIXELS);
    assert!(width <= 6000 && height <= 4000);
}

#[test]
fn shared_decode_budget_preserves_room_for_a_second_distinct_wallpaper() {
    let first_budget = remaining_image_budget(0, 2).unwrap();
    let first = scaled_source_size(4096, 4096, first_budget).unwrap();
    let first_pixels = u64::from(first.0) * u64::from(first.1);
    let second_budget = remaining_image_budget(first_pixels, 1).unwrap();
    let second = scaled_source_size(4096, 4096, second_budget).unwrap();
    let total_pixels = first_pixels + u64::from(second.0) * u64::from(second.1);

    assert_eq!(
        (first, second, total_pixels),
        ((2896, 2896), (2896, 2896), 16_773_632)
    );
}

#[test]
fn shared_decode_budget_rejects_an_empty_or_exhausted_allocation() {
    assert_eq!(remaining_image_budget(0, 0), None);
    assert_eq!(remaining_image_budget(MAX_IMAGE_PIXELS, 1), None);
    assert_eq!(remaining_image_budget(MAX_IMAGE_PIXELS + 1, 1), None);
    assert_eq!(remaining_image_budget(MAX_IMAGE_PIXELS - 1, 2), None);
}

#[test]
fn source_images_reject_zero_and_excessive_dimensions() {
    assert_eq!(scaled_source_size(0, 1, MAX_IMAGE_PIXELS), None);
    assert_eq!(scaled_source_size(1, 0, MAX_IMAGE_PIXELS), None);
    assert_eq!(
        scaled_source_size(u32::MAX, u32::MAX, MAX_IMAGE_PIXELS),
        None
    );
}

#[test]
fn the_current_decode_generation_is_accepted() {
    assert!(is_current_generation(7, 7));
}

#[test]
fn a_decode_finished_after_a_newer_request_is_rejected() {
    assert!(!is_current_generation(6, 7));
}

#[test]
fn an_unrequested_future_decode_generation_is_rejected() {
    assert!(!is_current_generation(8, 7));
}

#[test]
fn virtual_desktop_bounds_include_monitors_with_negative_origins() {
    let rect = union_rects([
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        RECT {
            left: -1280,
            top: -120,
            right: 0,
            bottom: 904,
        },
    ]);
    assert_eq!(
        (rect.left, rect.top, rect.right, rect.bottom),
        (-1280, -120, 1920, 1080)
    );
}

#[test]
fn virtual_desktop_bounds_do_not_include_an_unused_origin() {
    let rect = union_rects([
        RECT {
            left: 200,
            top: 300,
            right: 2120,
            bottom: 1380,
        },
        RECT {
            left: 2120,
            top: 500,
            right: 3400,
            bottom: 1524,
        },
        RECT {
            left: 400,
            top: 1380,
            right: 2320,
            bottom: 2460,
        },
    ]);
    assert_eq!(
        (rect.left, rect.top, rect.right, rect.bottom),
        (200, 300, 3400, 2460)
    );
}

#[test]
fn virtual_desktop_bounds_are_empty_without_monitors() {
    let rect = union_rects(std::iter::empty::<RECT>());
    assert_eq!((rect.left, rect.top, rect.right, rect.bottom), (0, 0, 0, 0));
}

#[test]
fn monitor_frame_destinations_are_relative_to_negative_virtual_origin() {
    let desktop = RECT {
        left: -1920,
        top: 0,
        right: 2560,
        bottom: 1440,
    };
    let left = frame_destination(
        RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        },
        desktop,
    );
    let primary = frame_destination(
        RECT {
            left: 0,
            top: 0,
            right: 2560,
            bottom: 1440,
        },
        desktop,
    );
    assert_eq!(
        (left.left, left.top, left.right, left.bottom),
        (0, 0, 1920, 1080)
    );
    assert_eq!(
        (primary.left, primary.top, primary.right, primary.bottom),
        (1920, 0, 4480, 1440)
    );
}

#[test]
fn only_successful_nonempty_monitor_rectangles_are_attached() {
    let rect = RECT {
        left: -1920,
        top: 0,
        right: 0,
        bottom: 1080,
    };
    assert!(is_attached_monitor(0, rect));
    assert!(!is_attached_monitor(1, rect));
    assert!(!is_attached_monitor(0x80004005u32 as i32, rect));
    assert!(!is_attached_monitor(
        0,
        RECT {
            left: 1920,
            top: 0,
            right: 1920,
            bottom: 1080
        }
    ));
}

#[test]
fn two_4k_monitor_images_fit_the_shared_decode_budget() {
    let first = scaled_source_size(3840, 2160, MAX_IMAGE_PIXELS).unwrap();
    let first_pixels = u64::from(first.0) * u64::from(first.1);
    let second = scaled_source_size(3840, 2160, MAX_IMAGE_PIXELS - first_pixels).unwrap();
    let total_pixels = first_pixels + u64::from(second.0) * u64::from(second.1);
    assert!(total_pixels <= MAX_IMAGE_PIXELS);
}

#[test]
fn alpha_composites_bgra_over_the_theme_color() {
    let mut transparent = [200, 100, 50, 0];
    composite_bgra(&mut transparent, [20, 40, 60]);
    assert_eq!(transparent, [20, 40, 60, 255]);

    let mut opaque = [200, 100, 50, 255];
    composite_bgra(&mut opaque, [20, 40, 60]);
    assert_eq!(opaque, [200, 100, 50, 255]);

    let mut translucent = [200, 100, 50, 128];
    composite_bgra(&mut translucent, [20, 40, 60]);
    assert_eq!(translucent, [110, 70, 55, 255]);
}

#[test]
fn center_keeps_native_source_dimensions_at_the_decode_budget() {
    assert_eq!(
        image_size_for_position(4096, 4096, 16_777_216, WallpaperPosition::Center),
        Some((4096, 4096))
    );
}

#[test]
fn tile_keeps_native_source_dimensions_at_the_decode_budget() {
    assert_eq!(
        image_size_for_position(4096, 4096, 16_777_216, WallpaperPosition::Tile),
        Some((4096, 4096))
    );
}

#[test]
fn center_rejects_a_source_that_would_need_downscaling() {
    assert_eq!(
        image_size_for_position(4096, 4096, 8_388_608, WallpaperPosition::Center),
        None
    );
}

#[test]
fn tile_rejects_a_source_that_would_need_downscaling() {
    assert_eq!(
        image_size_for_position(4096, 4096, 8_388_608, WallpaperPosition::Tile),
        None
    );
}

#[test]
fn fill_downscales_a_source_to_the_shared_decode_budget() {
    assert_eq!(
        image_size_for_position(4096, 4096, 8_388_608, WallpaperPosition::Fill),
        Some((2896, 2896))
    );
}

#[test]
fn unchanged_wallpaper_without_a_failed_decode_is_not_refreshed() {
    let now = std::time::Instant::now();
    assert!(!wallpaper_refresh_due(false, true, None, now));
}

#[test]
fn unchanged_failed_wallpaper_waits_until_its_retry_deadline() {
    let failed_at = std::time::Instant::now();
    let retry_at = failed_at + std::time::Duration::from_secs(2);
    assert!(!wallpaper_refresh_due(
        false,
        true,
        Some(retry_at),
        retry_at - std::time::Duration::from_nanos(1)
    ));
}

#[test]
fn unchanged_failed_wallpaper_can_retry_at_its_deadline() {
    let failed_at = std::time::Instant::now();
    let retry_at = failed_at + std::time::Duration::from_secs(2);
    assert!(wallpaper_refresh_due(false, true, Some(retry_at), retry_at));
}

#[test]
fn unchanged_failed_wallpaper_can_retry_after_its_deadline() {
    let failed_at = std::time::Instant::now();
    let retry_at = failed_at + std::time::Duration::from_secs(2);
    assert!(wallpaper_refresh_due(
        false,
        true,
        Some(retry_at),
        retry_at + std::time::Duration::from_secs(1)
    ));
}

#[test]
fn forced_wallpaper_refresh_bypasses_the_retry_deadline() {
    let now = std::time::Instant::now();
    let retry_at = now + std::time::Duration::from_secs(2);
    assert!(wallpaper_refresh_due(true, true, Some(retry_at), now));
}

#[test]
fn changed_wallpaper_path_bypasses_the_retry_deadline() {
    let now = std::time::Instant::now();
    let retry_at = now + std::time::Duration::from_secs(2);
    assert!(wallpaper_refresh_due(false, false, Some(retry_at), now));
}

fn decode_request(generation: u64) -> DecodeRequest {
    DecodeRequest {
        generation,
        path: Some(format!("wallpaper-{generation}.png").into()),
        rect: RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        color: COLORREF(0),
    }
}

#[test]
fn a_decode_queue_keeps_only_the_latest_pending_request() {
    let queue = RequestQueue::default();
    queue.submit(decode_request(1));
    queue.submit(decode_request(2));
    let request = queue.receive().unwrap();
    assert_eq!(request.generation, 2);
    assert_eq!(request.path, Some(PathBuf::from("wallpaper-2.png")));
}

#[test]
fn requests_submitted_around_repeated_worker_waits_are_not_stranded() {
    let queue = Arc::new(RequestQueue::default());
    let (ready_tx, ready_rx) = mpsc::channel();
    let (received_tx, received_rx) = mpsc::channel();
    let worker_queue = queue.clone();
    let worker = std::thread::spawn(move || loop {
        ready_tx.send(()).unwrap();
        let Some(request) = worker_queue.receive() else {
            break;
        };
        received_tx.send(request.generation).unwrap();
    });

    let timeout = std::time::Duration::from_secs(2);
    let mut received = Vec::new();
    for generation in 1..=32 {
        if ready_rx.recv_timeout(timeout).is_err() {
            break;
        }
        queue.submit(decode_request(generation));
        match received_rx.recv_timeout(timeout) {
            Ok(generation) => received.push(generation),
            Err(_) => break,
        }
    }
    queue.close();
    worker.join().unwrap();
    assert_eq!(received, (1..=32).collect::<Vec<_>>());
}

#[test]
fn closing_a_decode_queue_wakes_its_waiting_worker() {
    let queue = Arc::new(RequestQueue::default());
    let ready = Arc::new(std::sync::Barrier::new(2));
    let worker_queue = queue.clone();
    let worker_ready = ready.clone();
    let worker = std::thread::spawn(move || {
        worker_ready.wait();
        worker_queue.receive()
    });

    ready.wait();
    queue.close();
    assert!(worker.join().unwrap().is_none());
}

#[test]
fn native_size_wallpapers_share_the_remaining_budget_in_either_decode_order() {
    for position in [WallpaperPosition::Center, WallpaperPosition::Tile] {
        for sources in [[(4096, 3072), (1920, 1080)], [(1920, 1080), (4096, 3072)]] {
            let mut used_pixels = 0;
            for (index, (width, height)) in sources.into_iter().enumerate() {
                let budget = image_budget_for_position(used_pixels, 2 - index, position).unwrap();
                let size = image_size_for_position(width, height, budget, position).unwrap();
                assert_eq!(size, (width, height));
                used_pixels += u64::from(size.0) * u64::from(size.1);
            }
            assert_eq!(used_pixels, 14_656_512);
        }
    }
}

#[test]
fn fill_still_reserves_an_equal_budget_for_remaining_wallpapers() {
    assert_eq!(
        image_budget_for_position(0, 2, WallpaperPosition::Fill),
        Some(8_388_608)
    );
    assert_eq!(
        image_budget_for_position(2_073_600, 2, WallpaperPosition::Fill),
        Some(7_351_808)
    );
}

#[test]
fn fill_keeps_a_one_pixel_source_nonempty_on_a_landscape_monitor() {
    let crop = source_crop_fill(1, 1, 1920, 1080).unwrap();
    assert_eq!((crop.x, crop.y, crop.width, crop.height), (0, 0, 1, 1));
}

#[test]
fn fill_keeps_a_one_pixel_source_nonempty_on_a_portrait_monitor() {
    let crop = source_crop_fill(1, 1, 1080, 1920).unwrap();
    assert_eq!((crop.x, crop.y, crop.width, crop.height), (0, 0, 1, 1));
}

#[test]
fn factory_initialization_retries_com_after_a_failure() {
    let mut com_initialized = false;
    let mut factory = None;
    let mut initialization_attempts = 0;
    let mut creation_attempts = 0;
    for expected_success in [false, true] {
        let success = ensure_factory(
            &mut com_initialized,
            &mut factory,
            || {
                initialization_attempts += 1;
                initialization_attempts == 2
            },
            || {
                creation_attempts += 1;
                Some(42)
            },
        );
        assert_eq!(success, expected_success);
    }
    assert_eq!(
        (
            com_initialized,
            factory,
            initialization_attempts,
            creation_attempts
        ),
        (true, Some(42), 2, 1)
    );
}

#[test]
fn factory_initialization_retries_creation_without_reinitializing_com() {
    let mut com_initialized = false;
    let mut factory = None;
    let mut initialization_attempts = 0;
    let mut creation_attempts = 0;
    for expected_success in [false, true] {
        let success = ensure_factory(
            &mut com_initialized,
            &mut factory,
            || {
                initialization_attempts += 1;
                true
            },
            || {
                creation_attempts += 1;
                (creation_attempts == 2).then_some(42)
            },
        );
        assert_eq!(success, expected_success);
    }
    assert_eq!(
        (
            com_initialized,
            factory,
            initialization_attempts,
            creation_attempts
        ),
        (true, Some(42), 1, 2)
    );
}

#[test]
fn a_frozen_per_monitor_wallpaper_gives_way_to_the_applied_one() {
    let targets = |path: &str| vec![WallpaperTarget { rect: RECT::default(), path: Some(PathBuf::from(path)) }];
    let applied = std::path::Path::new(r"D:\themes\gruvbox\a.jpg");
    let mut stale = targets(r"D:\themes\osaka\b.jpg");
    assert!(replace_stale(&mut stale, Some(applied)));
    assert_eq!(stale[0].path.as_deref(), Some(applied));
    let mut same = targets(r"D:\Themes\Gruvbox\A.JPG");
    assert!(!replace_stale(&mut same, Some(applied)));
    let mut kept = targets(r"D:\themes\osaka\b.jpg");
    assert!(!replace_stale(&mut kept, Some(std::path::Path::new(r"C:\u\AppData\Roaming\Microsoft\Windows\Themes\TranscodedWallpaper"))));
    assert!(!replace_stale(&mut kept, None));
}
