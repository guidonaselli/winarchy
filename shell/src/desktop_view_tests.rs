use super::*;

struct ShortcutFixture {
    root: PathBuf,
}

impl ShortcutFixture {
    fn new(name: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "winarchy-folder-shortcut-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        Self { root }
    }

    fn save(&self, target: &std::path::Path) -> PathBuf {
        let shortcut = self.root.join("shortcut.lnk");
        let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        let path: Vec<u16> = shortcut.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            link.SetPath(PCWSTR(target.as_ptr())).unwrap();
            link.cast::<IPersistFile>()
                .unwrap()
                .Save(PCWSTR(path.as_ptr()), true)
                .unwrap();
        }
        shortcut
    }
}

impl Drop for ShortcutFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        unsafe { CoUninitialize() };
    }
}

#[test]
fn a_folder_shortcut_resolves_to_its_target() {
    let fixture = ShortcutFixture::new("folder");
    let target = fixture.root.join("Folder target");
    std::fs::create_dir(&target).unwrap();
    let shortcut = fixture.save(&target);

    assert_eq!(unsafe { folder_shortcut_target(&shortcut) }, Some(target));
}

#[test]
fn a_file_shortcut_keeps_the_shell_default() {
    let fixture = ShortcutFixture::new("file");
    let target = fixture.root.join("note.txt");
    std::fs::write(&target, b"note").unwrap();
    let shortcut = fixture.save(&target);

    assert_eq!(unsafe { folder_shortcut_target(&shortcut) }, None);
}

#[test]
fn a_shortcut_to_a_missing_folder_keeps_the_shell_default() {
    let fixture = ShortcutFixture::new("missing");
    let target = fixture.root.join("Missing folder");
    std::fs::create_dir(&target).unwrap();
    let shortcut = fixture.save(&target);
    std::fs::remove_dir(&target).unwrap();

    assert_eq!(unsafe { folder_shortcut_target(&shortcut) }, None);
}

#[test]
fn no_selection_ignores_the_default_command() {
    assert_eq!(default_command(0, false, None), DefaultCommand::Ignore);
}

#[test]
fn a_single_filesystem_folder_opens_in_files() {
    assert_eq!(
        default_command(1, true, Some(std::path::PathBuf::from(r"C:\Documents"))),
        DefaultCommand::OpenInFiles(std::path::PathBuf::from(r"C:\Documents")),
    );
}

#[test]
fn a_single_virtual_folder_is_rejected() {
    assert_eq!(
        default_command(1, true, None),
        DefaultCommand::UnsupportedVirtualFolder,
    );
}

#[test]
fn a_single_file_uses_the_shell_default() {
    assert_eq!(
        default_command(
            1,
            false,
            Some(std::path::PathBuf::from(r"C:\Documents\note.txt"))
        ),
        DefaultCommand::ShellDefault,
    );
}

#[test]
fn multiple_items_containing_a_folder_are_rejected() {
    assert_eq!(
        default_command(2, true, Some(std::path::PathBuf::from(r"C:\Documents"))),
        DefaultCommand::UnsupportedSelection,
    );
}

#[test]
fn multiple_files_use_the_shell_default() {
    assert_eq!(
        default_command(2, false, None),
        DefaultCommand::ShellDefault
    );
}

#[test]
fn a_successful_cli_exit_does_not_report_failure() {
    assert!(!process_exit_failed(Some(0)));
}

#[test]
fn a_nonzero_cli_exit_reports_failure() {
    assert!(process_exit_failed(Some(1)));
}

#[test]
fn a_cli_launch_error_reports_failure() {
    assert!(process_exit_failed(None));
}

#[test]
fn an_accelerator_is_consumed_for_exact_s_ok() {
    assert!(accelerator_consumed(windows::Win32::Foundation::S_OK));
}

#[test]
fn an_accelerator_is_not_consumed_for_s_false() {
    assert!(!accelerator_consumed(windows::Win32::Foundation::S_FALSE));
}

#[test]
fn an_accelerator_is_not_consumed_for_a_failure() {
    assert!(!accelerator_consumed(windows::Win32::Foundation::E_FAIL));
}
