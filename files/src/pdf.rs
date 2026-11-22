use std::{
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};
use windows::{
    Data::Pdf::PdfDocument,
    Storage::{
        StorageFile,
        Streams::{DataReader, InMemoryRandomAccessStream},
    },
    Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx},
    core::HSTRING,
};

// Renders page 1 to a cached PNG; None for unreadable or protected files.
pub fn first_page(path: &Path) -> Option<PathBuf> {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    std::fs::metadata(path).ok()?.modified().ok()?.hash(&mut hasher);
    let target = std::env::temp_dir().join(format!("winarchy-files-pdf-{:x}.png", hasher.finish()));
    if target.exists() {
        return Some(target);
    }
    // WinRT async results cannot be awaited on the UI thread, so a worker does the blocking calls.
    let source = path.to_owned();
    let output = target.clone();
    std::thread::spawn(move || render(&source, &output).ok()).join().ok()??;
    Some(target)
}

fn render(path: &Path, target: &Path) -> windows::core::Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path.as_os_str()))?.join()?;
    let page = PdfDocument::LoadFromFileAsync(&file)?.join()?.GetPage(0)?;
    let stream = InMemoryRandomAccessStream::new()?;
    page.RenderToStreamAsync(&stream)?.join()?;
    let size = stream.Size()? as u32;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    reader.LoadAsync(size)?.join()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;
    std::fs::write(target, bytes).map_err(|error| windows::core::Error::new(windows::core::HRESULT(-1), error.to_string()))
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "uses WinRT PDF rendering"]
    fn renders_a_minimal_pdf() {
        let dir = std::env::temp_dir().join(format!("wf-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("doc.pdf");
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] >>".to_owned(),
        ];
        let mut text = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(text.len());
            text.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
        }
        let xref = text.len();
        text.push_str("xref\n0 4\n0000000000 65535 f \n");
        for offset in &offsets {
            text.push_str(&format!("{offset:010} 00000 n \n"));
        }
        text.push_str(&format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"));
        std::fs::write(&pdf, text).unwrap();
        let out = dir.join("page.png");
        super::render(&pdf, &out).unwrap();
        assert!(std::fs::metadata(&out).unwrap().len() > 100);
        std::fs::remove_file(&pdf).unwrap();
        std::fs::remove_file(&out).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}
