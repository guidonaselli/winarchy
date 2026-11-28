#![windows_subsystem = "windows"]

mod contract;
mod controller;
mod desktop;
mod download;
mod memory;
mod orphans;
mod present;
mod redirect;
mod shell_window;
mod symbols;
mod views;
mod watch;
mod wrapper;

use contract::{Args, Failure, Report};
use std::{process::ExitCode, sync::mpsc, time::Duration};
use windows::{
    core::w,
    Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_INVALID_PARAMETER, HANDLE, WAIT_OBJECT_0},
        System::{
            Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
            Threading::{CreateMutexW, GetCurrentProcess, OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
        },
        UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
    },
};

const START_LIMIT: Duration = Duration::from_secs(30);

struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: This guard owns one open handle.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(args) = contract::parse_args(&args) else { return ExitCode::from(contract::EXIT_USAGE) };
    // SAFETY: No window exists yet in this process.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    // SAFETY: A named mutex held until the process ends.
    let mutex = unsafe { CreateMutexW(None, false, w!("Local\\winarchy-uwphost")) };
    // SAFETY: Read right after the call that sets it.
    let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let Ok(_mutex) = mutex.map(Owned) else { return ExitCode::from(contract::EXIT_ALREADY_RUNNING) };
    if exists {
        return ExitCode::from(contract::EXIT_ALREADY_RUNNING);
    }
    let report = Report::new(&args.root, symbols::file_version(symbols::TWINUI).unwrap_or_default());
    match run(&args, &report) {
        Ok(()) => {
            report.log("phase=exit code=0");
            report.status("stopped", "");
            ExitCode::from(contract::EXIT_OK)
        }
        Err(failure) => {
            report.log(&format!("phase=exit code={} error={}", failure.code(), failure.reason()));
            report.status("failed", failure.reason());
            ExitCode::from(failure.code())
        }
    }
}

fn run(args: &Args, report: &Report) -> Result<(), Failure> {
    let started = std::time::Instant::now();
    // SAFETY: Only the right to wait on the parent is requested.
    let parent = match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, args.parent) } {
        Ok(handle) => Owned(handle),
        Err(error) if error.code() == ERROR_INVALID_PARAMETER.to_hresult() => {
            report.log("phase=parent result=gone");
            return Ok(());
        }
        Err(error) => return Err(Failure::Runtime(format!("open parent: {error}"))),
    };
    report.status("starting", "");
    let log = |text: &str| report.log(text);
    // symbols and prologues come first: if either fails, the session is left untouched
    let symbols = symbols::load(&args.root, &|pdb| report.status("downloading", pdb), &log)?;
    wrapper::verify_prologues(&symbols)?;
    report.log(&format!("phase=symbols ms={}", started.elapsed().as_millis()));
    // the first download can outlive the shell that asked for it
    // SAFETY: A zero-timeout check of the handle opened above.
    if unsafe { WaitForSingleObject(parent.0, 0) } == WAIT_OBJECT_0 {
        report.log("phase=parent result=exited");
        return Ok(());
    }
    let preexisting = desktop::existing_cores();
    // a hang between registering the shell window and the watch loop would otherwise keep it forever
    let (started_up, deadline) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        if deadline.recv_timeout(START_LIMIT) == Err(mpsc::RecvTimeoutError::Timeout) {
            // SAFETY: TerminateProcess avoids DLL detach on a stuck thread.
            let _ = unsafe { TerminateProcess(GetCurrentProcess(), u32::from(contract::EXIT_WATCHDOG)) };
        }
    });
    let shell_window = shell_window::ShellWindow::register()?;
    // only once this host owns the session: otherwise nothing replaces what is closed
    orphans::close(&args.root, &log);
    // SAFETY: Balanced below, after every COM reference is dropped.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok().map_err(|e| Failure::Layout(format!("COM: {e}")))?;
    let result = (|| {
        let redirect = redirect::BandRedirect::install(&symbols)?;
        let controller = controller::start(&symbols)?;
        report.status("ready", "");
        report.log(&format!("phase=ready ms={}", started.elapsed().as_millis()));
        drop(started_up);
        let watched = watch::run(shell_window.0, parent.0, &symbols, preexisting, &args.root, &log);
        drop(controller);
        drop(redirect);
        watched
    })();
    // SAFETY: Matches the successful CoInitializeEx above.
    unsafe { CoUninitialize() };
    result
}
