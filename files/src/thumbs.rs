// Shell thumbnails and icons, rendered once to PNG files in a per-user cache so the UI can load them as plain images.
use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const EDGE: u32 = 96;

const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff", "heic", "avif", "ico", "mp4", "mkv", "avi", "mov", "wmv", "webm", "pdf", "doc", "docx", "xls", "xlsx",
    "ppt", "pptx", "exe", "lnk",
];

pub type Job = (PathBuf, Option<SystemTime>, u64);

pub fn wanted(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(extension)))
}

fn cache_file(path: &Path, modified: Option<SystemTime>, size: u64) -> Option<PathBuf> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (path, modified, size, EDGE).hash(&mut hasher);
    let folder = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("winarchy").join("thumbs");
    Some(folder.join(format!("{:016x}.png", hasher.finish())))
}

// The cached image of `job`, rendering it first when it is not there yet. Slow for a cold cache: call from a worker.
pub fn ensure(job: &Job) -> Option<PathBuf> {
    let (path, modified, size) = job;
    let file = cache_file(path, *modified, *size)?;
    if file.exists() {
        return Some(file);
    }
    let (width, height, rgba) = render(path)?;
    std::fs::create_dir_all(file.parent()?).ok()?;
    // written under a temporary name so a reader never sees half a file
    let partial = file.with_extension("tmp");
    std::fs::write(&partial, png(width, height, &rgba)).ok()?;
    std::fs::rename(&partial, &file).ok()?;
    Some(file)
}

#[cfg(windows)]
fn render(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    use windows::{
        Win32::{
            Foundation::SIZE,
            Graphics::Gdi::{BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, HGDIOBJ, ReleaseDC},
            System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_RESIZETOFIT},
        },
        core::HSTRING,
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).ok()?;
        let bitmap = factory.GetImage(SIZE { cx: EDGE as i32, cy: EDGE as i32 }, SIIGBF_RESIZETOFIT).ok()?;
        let mut info = BITMAP::default();
        GetObjectW(HGDIOBJ(bitmap.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut info as *mut _ as *mut _));
        let (width, height) = (info.bmWidth as u32, info.bmHeight as u32);
        let mut header = BITMAPINFO::default();
        header.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let screen = GetDC(None);
        let rows = GetDIBits(screen, bitmap, 0, height, Some(pixels.as_mut_ptr().cast()), &mut header, DIB_RGB_COLORS);
        ReleaseDC(None, screen);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        if rows == 0 {
            return None;
        }
        // BGRA with premultiplied alpha in, straight RGBA out
        for pixel in pixels.chunks_exact_mut(4) {
            let alpha = u32::from(pixel[3]);
            let (blue, green, red) = (u32::from(pixel[0]), u32::from(pixel[1]), u32::from(pixel[2]));
            let straight = |channel: u32| if alpha == 0 { 0 } else { (channel * 255 / alpha).min(255) as u8 };
            pixel.copy_from_slice(&[straight(red), straight(green), straight(blue), alpha as u8]);
        }
        Some((width, height, pixels))
    }
}

#[cfg(not(windows))]
fn render(_path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    None
}

// Uncompressed PNG: bigger than a deflated one, but needs no compression library and thumbnails are small.
fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for row in rgba.chunks_exact(width as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    while let Some(block) = blocks.next() {
        zlib.push(u8::from(blocks.peek().is_none()));
        zlib.extend((block.len() as u16).to_le_bytes());
        zlib.extend((!(block.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend(adler32(&raw).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut header = Vec::new();
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    header.extend([8, 6, 0, 0, 0]);
    for (kind, data) in [(b"IHDR", header), (b"IDAT", zlib), (b"IEND", Vec::new())] {
        out.extend((data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend(kind);
        out.extend(data);
        let checksum = crc32(&out[start..]);
        out.extend(checksum.to_be_bytes());
    }
    out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_the_reference_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn png_has_the_chunks_of_a_valid_file() {
        let image = png(2, 2, &[255; 16]);
        assert_eq!(&image[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&image[12..16], b"IHDR");
        assert_eq!(&image[image.len() - 8..image.len() - 4], b"IEND");
        assert_eq!(u32::from_be_bytes(image[29..33].try_into().unwrap()), crc32(&image[12..29]));
    }

    #[test]
    fn only_previewable_types_are_wanted() {
        assert!(wanted(Path::new(r"C:\a\photo.JPG")) && wanted(Path::new("tool.exe")));
        assert!(!wanted(Path::new("notes.txt")) && !wanted(Path::new("noextension")));
    }
}
