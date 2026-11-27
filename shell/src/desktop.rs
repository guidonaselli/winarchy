use crate::desktop_view;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use windows::core::{w, Interface, GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    IDesktopWallpaper, DWPOS_CENTER, DWPOS_FILL, DWPOS_FIT, DWPOS_SPAN, DWPOS_STRETCH, DWPOS_TILE,
};
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_WALLPAPER_READY: u32 = WM_APP + 90;
const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_SOURCE_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_INPUT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WallpaperPosition {
    Center,
    Tile,
    Stretch,
    Fit,
    Fill,
    Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImageLayout {
    source: SourceRect,
    destination: RECT,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn source_crop_fill(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> Option<SourceRect> {
    if source_width == 0 || source_height == 0 || target_width == 0 || target_height == 0 {
        return None;
    }
    let source_ratio = u64::from(source_width) * u64::from(target_height);
    let target_ratio = u64::from(target_width) * u64::from(source_height);
    if source_ratio > target_ratio {
        let width = (u64::from(source_height) * u64::from(target_width) / u64::from(target_height))
            .max(1) as u32;
        Some(SourceRect {
            x: (source_width - width) / 2,
            y: 0,
            width,
            height: source_height,
        })
    } else if source_ratio < target_ratio {
        let height = (u64::from(source_width) * u64::from(target_height) / u64::from(target_width))
            .max(1) as u32;
        Some(SourceRect {
            x: 0,
            y: (source_height - height) / 2,
            width: source_width,
            height,
        })
    } else {
        Some(SourceRect {
            x: 0,
            y: 0,
            width: source_width,
            height: source_height,
        })
    }
}

fn frame_layout(
    position: WallpaperPosition,
    image_width: u32,
    image_height: u32,
    monitor: RECT,
    desktop: RECT,
) -> Option<ImageLayout> {
    if image_width == 0 || image_height == 0 {
        return None;
    }
    let destination = frame_destination(monitor, desktop);
    let width = u64::try_from(destination.right.checked_sub(destination.left)?).ok()?;
    let height = u64::try_from(destination.bottom.checked_sub(destination.top)?).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let full_source = SourceRect {
        x: 0,
        y: 0,
        width: image_width,
        height: image_height,
    };
    let (source, destination) = match position {
        WallpaperPosition::Tile => return None,
        WallpaperPosition::Center => {
            let source_width = image_width.min(width as u32);
            let source_height = image_height.min(height as u32);
            let source = SourceRect {
                x: (image_width - source_width) / 2,
                y: (image_height - source_height) / 2,
                width: source_width,
                height: source_height,
            };
            let left = destination.left + ((width - u64::from(source_width)) / 2) as i32;
            let top = destination.top + ((height - u64::from(source_height)) / 2) as i32;
            (
                source,
                RECT {
                    left,
                    top,
                    right: left + source_width as i32,
                    bottom: top + source_height as i32,
                },
            )
        }
        WallpaperPosition::Fit => {
            let image_width = u64::from(image_width);
            let image_height = u64::from(image_height);
            let (fit_width, fit_height) = if image_width * height > image_height * width {
                (width, (image_height * width / image_width).max(1))
            } else {
                ((image_width * height / image_height).max(1), height)
            };
            let left = destination.left + ((width - fit_width) / 2) as i32;
            let top = destination.top + ((height - fit_height) / 2) as i32;
            (
                full_source,
                RECT {
                    left,
                    top,
                    right: left + fit_width as i32,
                    bottom: top + fit_height as i32,
                },
            )
        }
        WallpaperPosition::Fill | WallpaperPosition::Span => (
            source_crop_fill(image_width, image_height, width as u32, height as u32)?,
            destination,
        ),
        WallpaperPosition::Stretch => (full_source, destination),
    };
    Some(ImageLayout {
        source,
        destination,
    })
}

fn valid_image_size(width: u32, height: u32) -> Option<usize> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return None;
    }
    (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)
}

fn scaled_source_size(width: u32, height: u32, max_pixels: u64) -> Option<(u32, u32)> {
    let pixels = u64::from(width).checked_mul(u64::from(height))?;
    if width == 0 || height == 0 || pixels > MAX_SOURCE_PIXELS || max_pixels == 0 {
        return None;
    }
    if pixels <= max_pixels {
        return Some((width, height));
    }
    let scale = (max_pixels as f64 / pixels as f64).sqrt();
    Some((
        ((width as f64 * scale).floor() as u32).max(1).min(width),
        ((height as f64 * scale).floor() as u32).max(1).min(height),
    ))
}

fn image_size_for_position(
    width: u32,
    height: u32,
    budget: u64,
    position: WallpaperPosition,
) -> Option<(u32, u32)> {
    if matches!(
        position,
        WallpaperPosition::Center | WallpaperPosition::Tile
    ) {
        let pixels = u64::from(width).checked_mul(u64::from(height))?;
        if width == 0 || height == 0 || pixels > MAX_SOURCE_PIXELS || pixels > budget {
            return None;
        }
        Some((width, height))
    } else {
        scaled_source_size(width, height, budget)
    }
}

fn image_budget_for_position(
    used_pixels: u64,
    remaining_images: usize,
    position: WallpaperPosition,
) -> Option<u64> {
    match position {
        WallpaperPosition::Center | WallpaperPosition::Tile => {
            if used_pixels >= MAX_IMAGE_PIXELS || remaining_images == 0 {
                None
            } else {
                Some(MAX_IMAGE_PIXELS - used_pixels)
            }
        }
        WallpaperPosition::Stretch
        | WallpaperPosition::Fit
        | WallpaperPosition::Fill
        | WallpaperPosition::Span => remaining_image_budget(used_pixels, remaining_images),
    }
}

fn ensure_factory<T>(
    com_initialized: &mut bool,
    factory: &mut Option<T>,
    initialize: impl FnOnce() -> bool,
    create: impl FnOnce() -> Option<T>,
) -> bool {
    if !*com_initialized {
        *com_initialized = initialize();
    }
    if *com_initialized && factory.is_none() {
        *factory = create();
    }
    factory.is_some()
}

fn wallpaper_refresh_due(
    force_refresh: bool,
    same_path: bool,
    retry_at: Option<Instant>,
    now: Instant,
) -> bool {
    force_refresh || !same_path || retry_at.is_some_and(|retry_at| now >= retry_at)
}

fn remaining_image_budget(used_pixels: u64, remaining_images: usize) -> Option<u64> {
    if used_pixels >= MAX_IMAGE_PIXELS || remaining_images == 0 {
        return None;
    }
    let budget = (MAX_IMAGE_PIXELS - used_pixels) / remaining_images as u64;
    (budget > 0).then_some(budget)
}

fn is_current_generation(result: u64, current: u64) -> bool {
    result == current
}

struct Image {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    tile_brush: Option<usize>,
}

impl Drop for Image {
    fn drop(&mut self) {
        if let Some(brush) = self.tile_brush {
            unsafe {
                let _ = DeleteObject(HBRUSH(brush as *mut _));
            }
        }
    }
}

struct Frame {
    rect: RECT,
    image: Option<Arc<Image>>,
}

struct WallpaperTarget {
    rect: RECT,
    path: Option<PathBuf>,
}

struct DecodeRequest {
    generation: u64,
    path: Option<PathBuf>,
    rect: RECT,
    color: COLORREF,
}

struct DecodeResult {
    generation: u64,
    image: Result<(Vec<Frame>, COLORREF, WallpaperPosition), ()>,
}

#[derive(Default)]
struct RequestState {
    pending: Option<DecodeRequest>,
    closed: bool,
}

#[derive(Default)]
struct RequestQueue {
    state: Mutex<RequestState>,
    ready: Condvar,
}

impl RequestQueue {
    fn submit(&self, request: DecodeRequest) {
        let mut state = self.state.lock().unwrap();
        if !state.closed {
            state.pending = Some(request);
            self.ready.notify_one();
        }
    }

    fn receive(&self) -> Option<DecodeRequest> {
        let mut state = self.state.lock().unwrap();
        loop {
            if let Some(request) = state.pending.take() {
                return Some(request);
            }
            if state.closed {
                return None;
            }
            state = self.ready.wait(state).unwrap();
        }
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        state.pending = None;
        self.ready.notify_one();
    }
}

struct State {
    hwnd: HWND,
    rect: RECT,
    color: COLORREF,
    position: WallpaperPosition,
    frames: Vec<Frame>,
    stamp: Option<PathBuf>,
    retry_at: Option<Instant>,
    generation: u64,
    force_refresh: bool,
    requests: Arc<RequestQueue>,
    results: Receiver<DecodeResult>,
}

thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }

fn union_rects(rects: impl IntoIterator<Item = RECT>) -> RECT {
    let mut rects = rects.into_iter();
    let Some(mut rect) = rects.next() else {
        return RECT::default();
    };
    for next in rects {
        rect.left = rect.left.min(next.left);
        rect.top = rect.top.min(next.top);
        rect.right = rect.right.max(next.right);
        rect.bottom = rect.bottom.max(next.bottom);
    }
    rect
}

fn desktop_rect() -> RECT {
    union_rects(unsafe { crate::monitor_rects() })
}

struct TaskMemString(*mut u16);

impl TaskMemString {
    fn is_null(&self) -> bool {
        self.0.is_null()
    }
}

impl Drop for TaskMemString {
    fn drop(&mut self) {
        if !self.is_null() {
            unsafe { CoTaskMemFree(Some(self.0.cast())) }
        }
    }
}

fn is_attached_monitor(rect_hr: i32, rect: RECT) -> bool {
    rect_hr == 0 && rect.right > rect.left && rect.bottom > rect.top
}

unsafe fn monitor_wallpaper(
    wallpaper: &IDesktopWallpaper,
    index: u32,
) -> Result<Option<WallpaperTarget>, ()> {
    let vtable = IDesktopWallpaper::vtable(wallpaper);
    let raw = wallpaper.as_raw();
    let mut id = PWSTR::null();
    let id_hr = (vtable.GetMonitorDevicePathAt)(raw, index, &mut id);
    let id = TaskMemString(id.0);
    if id_hr.0 != 0 || id.is_null() {
        return Err(());
    }
    let monitor_id = PWSTR(id.0).to_string().map_err(|_| ())?;
    if monitor_id.is_empty() {
        return Err(());
    }
    let monitor_id: Vec<u16> = monitor_id.encode_utf16().chain(Some(0)).collect();
    let monitor_id = PCWSTR(monitor_id.as_ptr());
    let mut rect = RECT::default();
    let rect_hr = (vtable.GetMonitorRECT)(raw, monitor_id, &mut rect);
    if !is_attached_monitor(rect_hr.0, rect) {
        return Ok(None);
    }
    let mut path = PWSTR::null();
    let path_hr = (vtable.GetWallpaper)(raw, monitor_id, &mut path);
    let path = TaskMemString(path.0);
    if path_hr.0 != 0 {
        return Err(());
    }
    let path = if path.is_null() {
        String::new()
    } else {
        PWSTR(path.0).to_string().map_err(|_| ())?
    };
    Ok(Some(WallpaperTarget {
        rect,
        path: (!path.is_empty()).then(|| PathBuf::from(path)),
    }))
}

// Con un explorer.exe que no es la shell, IDesktopWallpaper queda congelado: manda el fondo aplicado por SystemParametersInfo.
fn replace_stale(targets: &mut [WallpaperTarget], applied: Option<&std::path::Path>) -> bool {
    let Some(applied) = applied else { return false };
    let key = |path: &std::path::Path| path.to_string_lossy().to_lowercase();
    let wanted = key(applied);
    // TranscodedWallpaper y las presentaciones viven en Themes: ahí el valor por monitor es el bueno.
    if wanted.contains(r"\microsoft\windows\themes\") || targets.iter().any(|t| t.path.as_deref().is_some_and(|p| key(p) == wanted)) {
        return false;
    }
    for target in targets.iter_mut() {
        target.path = Some(applied.to_path_buf());
    }
    !targets.is_empty()
}

unsafe fn desktop_wallpapers() -> Option<(Vec<WallpaperTarget>, COLORREF, WallpaperPosition)> {
    let class = GUID::from_values(
        0xC2CF3110,
        0x460E,
        0x4FC1,
        [0xB9, 0xD0, 0x8A, 0x1C, 0x0C, 0x9C, 0xC4, 0xBD],
    );
    let wallpaper: IDesktopWallpaper = CoCreateInstance(&class, None, CLSCTX_LOCAL_SERVER).ok()?;
    let position = match wallpaper.GetPosition().ok()? {
        DWPOS_CENTER => WallpaperPosition::Center,
        DWPOS_TILE => WallpaperPosition::Tile,
        DWPOS_STRETCH => WallpaperPosition::Stretch,
        DWPOS_FIT => WallpaperPosition::Fit,
        DWPOS_FILL => WallpaperPosition::Fill,
        DWPOS_SPAN => WallpaperPosition::Span,
        _ => return None,
    };
    let color = wallpaper.GetBackgroundColor().ok()?;
    if position == WallpaperPosition::Span {
        let vtable = IDesktopWallpaper::vtable(&wallpaper);
        let mut path = PWSTR::null();
        let path_hr = (vtable.GetWallpaper)(wallpaper.as_raw(), PCWSTR::null(), &mut path);
        let path = TaskMemString(path.0);
        if path_hr.0 != 0 {
            return None;
        }
        let path = if path.is_null() {
            String::new()
        } else {
            PWSTR(path.0).to_string().ok()?
        };
        return Some((
            vec![WallpaperTarget {
                rect: desktop_rect(),
                path: (!path.is_empty()).then(|| PathBuf::from(path)),
            }],
            color,
            position,
        ));
    }
    let count = wallpaper.GetMonitorDevicePathCount().ok()?;
    if count == 0 || count > 64 {
        return None;
    }
    let mut targets = Vec::with_capacity(count as usize);
    for index in 0..count {
        if let Some(target) = monitor_wallpaper(&wallpaper, index).ok()? {
            targets.push(target);
        }
    }
    (!targets.is_empty()).then_some((targets, color, position))
}

unsafe fn wallpaper_path() -> windows::core::Result<PathBuf> {
    let mut buffer = [0u16; 32768];
    SystemParametersInfoW(
        SPI_GETDESKWALLPAPER,
        buffer.len() as u32,
        Some(buffer.as_mut_ptr().cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    )?;
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..end])))
}

fn worker(hwnd: isize, requests: Arc<RequestQueue>, results: Sender<DecodeResult>, root: PathBuf) {
    unsafe {
        let mut com_initialized = false;
        let mut factory: Option<IWICImagingFactory> = None;
        while let Some(request) = requests.receive() {
            ensure_factory(
                &mut com_initialized,
                &mut factory,
                || CoInitializeEx(None, COINIT_MULTITHREADED).is_ok(),
                || CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok(),
            );
            let generation = request.generation;
            let wanted = request.path.clone();
            let image = factory
                .as_ref()
                .map_or(Err(()), |factory| decode_targets(factory, request, &root));
            if image.is_err() {
                crate::boot_log(&root, &format!("wallpaper: decode failed for {wanted:?}"));
            }
            if results.send(DecodeResult { generation, image }).is_err() {
                break;
            }
            let _ = PostMessageW(
                HWND(hwnd as *mut _),
                WM_WALLPAPER_READY,
                WPARAM(0),
                LPARAM(0),
            );
        }
        drop(factory);
        if com_initialized {
            CoUninitialize();
        }
    }
}

fn composite_bgra(pixel: &mut [u8], background: [u8; 3]) {
    let alpha = u32::from(pixel[3]);
    for (channel, background) in pixel[..3].iter_mut().zip(background) {
        *channel = ((u32::from(*channel) * alpha + u32::from(background) * (255 - alpha) + 127)
            / 255) as u8;
    }
    pixel[3] = 255;
}

fn create_tile_brush(width: u32, height: u32, pixels: &[u8]) -> Option<usize> {
    let size = valid_image_size(width, height)?;
    if size != pixels.len() {
        return None;
    }
    let header = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: i32::try_from(width).ok()?,
        biHeight: -i32::try_from(height).ok()?,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: u32::try_from(size).ok()?,
        ..Default::default()
    };
    let header_bytes = unsafe {
        std::slice::from_raw_parts(
            (&header as *const BITMAPINFOHEADER).cast::<u8>(),
            std::mem::size_of::<BITMAPINFOHEADER>(),
        )
    };
    let mut packed = Vec::new();
    packed
        .try_reserve_exact(header_bytes.len().checked_add(size)?)
        .ok()?;
    packed.extend_from_slice(header_bytes);
    packed.extend_from_slice(pixels);
    let brush = unsafe { CreateDIBPatternBrushPt(packed.as_ptr().cast(), DIB_RGB_COLORS) };
    (!brush.0.is_null()).then_some(brush.0 as usize)
}

unsafe fn decode(
    factory: &IWICImagingFactory,
    path: &std::path::Path,
    color: COLORREF,
    max_pixels: u64,
    position: WallpaperPosition,
) -> Result<Image, ()> {
    let metadata = std::fs::metadata(path).map_err(|_| ())?;
    if metadata.len() == 0 || metadata.len() > MAX_INPUT_BYTES {
        return Err(());
    }
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let decoder = factory
        .CreateDecoderFromFilename(
            PCWSTR(path.as_ptr()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )
        .map_err(|_| ())?;
    let frame = decoder.GetFrame(0).map_err(|_| ())?;
    let (mut source_width, mut source_height) = (0, 0);
    frame
        .GetSize(&mut source_width, &mut source_height)
        .map_err(|_| ())?;
    let (width, height) =
        image_size_for_position(source_width, source_height, max_pixels, position).ok_or(())?;
    let size = valid_image_size(width, height).ok_or(())?;
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(size).map_err(|_| ())?;
    pixels.resize(size, 0);
    let converter = factory.CreateFormatConverter().map_err(|_| ())?;
    if width == source_width && height == source_height {
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|_| ())?;
    } else {
        let scaler = factory.CreateBitmapScaler().map_err(|_| ())?;
        scaler
            .Initialize(&frame, width, height, WICBitmapInterpolationModeFant)
            .map_err(|_| ())?;
        converter
            .Initialize(
                &scaler,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|_| ())?;
    }
    converter
        .CopyPixels(
            std::ptr::null(),
            width.checked_mul(4).ok_or(())?,
            &mut pixels,
        )
        .map_err(|_| ())?;
    let background = [(color.0 >> 16) as u8, (color.0 >> 8) as u8, color.0 as u8];
    for pixel in pixels.chunks_exact_mut(4) {
        composite_bgra(pixel, background);
    }
    let tile_brush = if position == WallpaperPosition::Tile {
        Some(create_tile_brush(width, height, &pixels).ok_or(())?)
    } else {
        None
    };
    Ok(Image {
        width,
        height,
        pixels,
        tile_brush,
    })
}

unsafe fn decode_targets(
    factory: &IWICImagingFactory,
    request: DecodeRequest,
    root: &std::path::Path,
) -> Result<(Vec<Frame>, COLORREF, WallpaperPosition), ()> {
    let fallback_color = request.color;
    let fallback = WallpaperTarget {
        rect: request.rect,
        path: request.path,
    };
    let (mut targets, color, position) = desktop_wallpapers().unwrap_or_else(|| {
        (
            fallback
                .path
                .iter()
                .cloned()
                .map(|path| WallpaperTarget {
                    rect: fallback.rect,
                    path: Some(path),
                })
                .collect(),
            fallback_color,
            WallpaperPosition::Fill,
        )
    });
    if replace_stale(&mut targets, fallback.path.as_deref().filter(|path| path.is_file())) {
        crate::boot_log(root, &format!("wallpaper: per-monitor API is stale, using {}", fallback.path.as_deref().unwrap_or(root).display()));
    }
    let mut remaining_images = targets
        .iter()
        .filter_map(|target| target.path.as_ref())
        .collect::<HashSet<_>>()
        .len();
    let mut decoded = HashMap::<PathBuf, Arc<Image>>::new();
    let mut frames = Vec::with_capacity(targets.len());
    let mut total_pixels = 0u64;
    let mut total_bytes = 0u64;
    for target in targets {
        let image = if let Some(path) = target.path {
            if let Some(image) = decoded.get(&path) {
                Some(image.clone())
            } else {
                let bytes = std::fs::metadata(&path).map_err(|_| ())?.len();
                total_bytes = total_bytes.checked_add(bytes).ok_or(())?;
                if bytes == 0 || total_bytes > MAX_INPUT_BYTES {
                    return Err(());
                }
                let image_budget =
                    image_budget_for_position(total_pixels, remaining_images, position).ok_or(())?;
                let image = Arc::new(decode(factory, &path, color, image_budget, position)?);
                total_pixels = total_pixels
                    .checked_add(u64::from(image.width) * u64::from(image.height))
                    .ok_or(())?;
                remaining_images -= 1;
                decoded.insert(path, image.clone());
                Some(image)
            }
        } else {
            None
        };
        frames.push(Frame {
            rect: target.rect,
            image,
        });
    }
    Ok((frames, color, position))
}

fn frame_destination(monitor: RECT, desktop: RECT) -> RECT {
    RECT {
        left: monitor.left.saturating_sub(desktop.left),
        top: monitor.top.saturating_sub(desktop.top),
        right: monitor.right.saturating_sub(desktop.left),
        bottom: monitor.bottom.saturating_sub(desktop.top),
    }
}

unsafe fn paint_background(dc: HDC, clip: RECT) {
    let (desktop, color, position, frames) = STATE
        .with(|state| {
            state.borrow().as_ref().map(|state| {
                (
                    state.rect,
                    state.color,
                    state.position,
                    state.frames
                        .iter()
                        .map(|frame| (frame.rect, frame.image.clone()))
                        .collect::<Vec<_>>(),
                )
            })
        })
        .unwrap_or_else(|| {
            (
                RECT::default(),
                COLORREF(0),
                WallpaperPosition::Fill,
                Vec::new(),
            )
        });
    let brush = CreateSolidBrush(color);
    let _ = FillRect(dc, &clip, brush);
    let _ = DeleteObject(brush);
    for (monitor, image) in frames {
        if let Some(image) = image {
            draw_frame(dc, desktop, monitor, &image, position);
        }
    }
}

unsafe fn draw_frame(
    dc: HDC,
    desktop: RECT,
    monitor: RECT,
    image: &Image,
    position: WallpaperPosition,
) {
    if position == WallpaperPosition::Tile {
        if let Some(brush) = image.tile_brush {
            let target = frame_destination(monitor, desktop);
            let _ = FillRect(dc, &target, HBRUSH(brush as *mut _));
        }
        return;
    }
    let Some(layout) = frame_layout(position, image.width, image.height, monitor, desktop) else {
        return;
    };
    let target_width = layout.destination.right - layout.destination.left;
    let target_height = layout.destination.bottom - layout.destination.top;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: image.width as i32,
            biHeight: -(image.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let _ = StretchDIBits(
        dc,
        layout.destination.left,
        layout.destination.top,
        target_width,
        target_height,
        layout.source.x as i32,
        layout.source.y as i32,
        layout.source.width as i32,
        layout.source.height as i32,
        Some(image.pixels.as_ptr().cast()),
        &info,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        // Activable para recibir el teclado (Supr, F2), pero activarse no debe subirla sobre las ventanas.
        WM_WINDOWPOSCHANGING => {
            let pos = &mut *(lp.0 as *mut WINDOWPOS);
            pos.hwndInsertAfter = HWND_BOTTOM;
            pos.flags &= !SWP_NOZORDER;
            LRESULT(0)
        }
        desktop_view::WM_FILES_OPEN_FAILED => {
            desktop_view::report_files_open_failure(hwnd, wp);
            LRESULT(0)
        }
        WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR => {
            desktop_view::handle_menu_message(message, wp, lp)
                .unwrap_or_else(|| DefWindowProcW(hwnd, message, wp, lp))
        }
        WM_ERASEBKGND => {
            let mut client = RECT::default();
            let _ = GetClientRect(hwnd, &mut client);
            paint_background(HDC(wp.0 as *mut _), client);
            LRESULT(1)
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut paint);
            if !dc.is_invalid() {
                paint_background(dc, paint.rcPaint);
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_WALLPAPER_READY => {
            let changed = STATE.with(|state| {
                let mut state = state.borrow_mut();
                let Some(state) = state.as_mut() else {
                    return false;
                };
                let mut changed = false;
                while let Ok(result) = state.results.try_recv() {
                    if is_current_generation(result.generation, state.generation) {
                        if let Ok((frames, color, position)) = result.image {
                            state.frames = frames;
                            state.color = color;
                            state.position = position;
                            state.retry_at = None;
                            changed = true;
                        } else {
                            state.retry_at = Some(Instant::now() + Duration::from_secs(2));
                        }
                    }
                }
                changed
            });
            if changed {
                let _ = RedrawWindow(
                    hwnd,
                    None,
                    None,
                    RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
                );
            }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            reposition();
            force_refresh();
            refresh();
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            force_refresh();
            refresh();
            LRESULT(0)
        }
        WM_THEMECHANGED | WM_SYSCOLORCHANGE => {
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, message, wp, lp),
    }
}

fn force_refresh() {
    STATE.with(|state| {
        if let Some(state) = state.borrow_mut().as_mut() {
            state.force_refresh = true;
        }
    });
}

fn reposition() {
    let rect = desktop_rect();
    let hwnd = STATE.with(|state| state.borrow().as_ref().map(|state| state.hwnd));
    if let Some(hwnd) = hwnd {
        if rect.right > rect.left && rect.bottom > rect.top {
            let _ = unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_BOTTOM,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            };
            STATE.with(|state| {
                if let Some(state) = state.borrow_mut().as_mut() {
                    state.rect = rect;
                }
            });
            let _ = unsafe { InvalidateRect(hwnd, None, false) };
            desktop_view::resize(RECT {
                left: 0,
                top: 0,
                right: rect.right - rect.left,
                bottom: rect.bottom - rect.top,
            });
            desktop_view::invalidate();
        }
    }
}

pub fn set_fallback_color(hex: &str) {
    let (r, g, b) = crate::theme::rgb(hex);
    let color = COLORREF(u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16);
    let update = STATE.with(|state| {
        state.borrow_mut().as_mut().map(|state| {
            let changed = state.color != color;
            state.color = color;
            (state.hwnd, changed)
        })
    });
    if let Some((hwnd, changed)) = update {
        if changed {
            force_refresh();
            refresh();
        }
        unsafe {
            let _ = RedrawWindow(
                hwnd,
                None,
                None,
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
            );
        }
    }
}

// user32!SetShellWindow no está documentada ni en la crate `windows`: se resuelve en runtime y, si falta, el escritorio sigue sin registrarse.
unsafe fn register_shell_window(hwnd: HWND) {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    let Ok(user32) = LoadLibraryW(w!("user32.dll")) else { return };
    let Some(proc) = GetProcAddress(user32, windows::core::s!("SetShellWindow")) else { return };
    let set: unsafe extern "system" fn(HWND) -> BOOL = std::mem::transmute(proc);
    let _ = set(hwnd);
}

pub fn init(fallback: &str, root: &std::path::Path, com_initialized: bool) {
    let rect = desktop_rect();
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return;
    }
    let Ok(instance) = (unsafe { GetModuleHandleW(PCWSTR::null()) }) else {
        return;
    };
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        lpszClassName: w!("WinarchyDesktopWallpaper"),
        ..Default::default()
    };
    unsafe {
        let _ = RegisterClassW(&class);
    }
    let Ok(hwnd) = (unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.lpszClassName,
            w!(""),
            WS_POPUP | WS_CLIPCHILDREN,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            instance,
            None,
        )
    }) else {
        return;
    };
    let requests = Arc::new(RequestQueue::default());
    let worker_requests = requests.clone();
    let (result_tx, result_rx) = mpsc::channel();
    let notify = hwnd.0 as isize;
    let log_root = root.to_path_buf();
    std::thread::spawn(move || worker(notify, worker_requests, result_tx, log_root));
    let (r, g, b) = crate::theme::rgb(fallback);
    STATE.with(|state| {
        *state.borrow_mut() = Some(State {
            hwnd,
            rect,
            color: COLORREF(u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16),
            position: WallpaperPosition::Fill,
            frames: Vec::new(),
            stamp: None,
            retry_at: None,
            generation: 0,
            force_refresh: true,
            requests,
            results: result_rx,
        });
    });
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND_BOTTOM,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        // Como Progman con Explorer: sin ventana de shell registrada, lo que komorebi manda al fondo queda debajo del escritorio y desaparece.
        register_shell_window(hwnd);
    }
    if com_initialized {
        let size = RECT {
            left: 0,
            top: 0,
            right: rect.right - rect.left,
            bottom: rect.bottom - rect.top,
        };
        if let Err(error) = desktop_view::init(hwnd, size, root.to_path_buf()) {
            crate::boot_log(root, &format!("desktop view initialization failed: {error:?}"));
        }
    }
    refresh();
}

pub fn refresh() {
    if !STATE.with(|state| state.borrow().is_some()) {
        return;
    }
    let path = unsafe { wallpaper_path().ok() }.filter(|path| !path.as_os_str().is_empty());
    let hwnd = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.as_mut() else {
            return None;
        };
        if !wallpaper_refresh_due(
            state.force_refresh,
            state.stamp == path,
            state.retry_at,
            Instant::now(),
        ) {
            return None;
        }
        state.force_refresh = false;
        state.stamp = path.clone();
        state.retry_at = None;
        state.generation = state.generation.wrapping_add(1);
        state.requests.submit(DecodeRequest {
            generation: state.generation,
            path,
            rect: state.rect,
            color: state.color,
        });
        Some(state.hwnd)
    });
    if let Some(hwnd) = hwnd {
        unsafe {
            let _ = InvalidateRect(hwnd, None, false);
        }
    }
}

// Un explorer.exe que convive con la shell registrada deja su Progman como ventana común: sube sobre las apps y se come los clics.
// shortcut: si la shell muere a la fuerza queda oculto hasta que otra shell cierre en orden o se reinicie Explorer.
fn show_foreign_desktops(show: bool) {
    unsafe {
        let mut previous = HWND::default();
        while let Ok(progman) = FindWindowExW(None, previous, w!("Progman"), PCWSTR::null()) {
            previous = progman;
            if IsWindowVisible(progman).as_bool() != show {
                let _ = ShowWindowAsync(progman, if show { SW_SHOWNA } else { SW_HIDE });
            }
        }
    }
}

pub fn tick() {
    if STATE.with(|state| state.borrow().as_ref().is_some_and(|state| unsafe { GetShellWindow() } == state.hwnd)) {
        show_foreign_desktops(false);
    }
    refresh();
}

pub fn shutdown() {
    desktop_view::shutdown();
    let state = STATE.with(|state| state.borrow_mut().take());
    if let Some(state) = state {
        show_foreign_desktops(true);
        state.requests.close();
        let hwnd = state.hwnd;
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

pub fn is_host_window(hwnd: HWND) -> bool {
    STATE.with(|state| {
        state
            .borrow()
            .as_ref()
            .is_some_and(|state| state.hwnd == hwnd)
    })
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;
