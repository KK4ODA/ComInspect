//! Starting a program the user picked, e.g. "start VarAC when COM4 is free".

use std::path::Path;

/// Starts a program, shortcut or app bundle with its own folder as the
/// working directory (many radio programs look for their settings there),
/// and returns without waiting for it to exit.
pub fn launch_program(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    if !path.exists() {
        return Err(format!("{} was not found", path.display()));
    }
    launch(path)
}

#[cfg(windows)]
fn launch(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }
    let file = wide(path.as_os_str());
    let dir = path.parent().map(|p| wide(p.as_os_str()));
    let display = path.display().to_string();
    // The shell may use COM (shortcuts), so call it from a thread with COM
    // initialized as it expects.
    std::thread::spawn(move || {
        let verb: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: initializes COM for this new thread only; balanced below.
        let com = unsafe {
            CoInitializeEx(
                std::ptr::null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
            )
        };
        // SAFETY: valid NUL-terminated strings that outlive the call.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
                SW_SHOWNORMAL,
            )
        };
        if com >= 0 {
            // SAFETY: balances the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
        }
        // ShellExecute reports success with a value greater than 32.
        let code = result as isize;
        if code > 32 {
            Ok(())
        } else {
            Err(format!("Windows could not start {display} (error {code})"))
        }
    })
    .join()
    .unwrap_or_else(|_| Err("starting the program failed".into()))
}

#[cfg(unix)]
fn launch(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    let is_app_bundle = path.extension().is_some_and(|e| e == "app");
    let executable = path.is_file()
        && std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0);
    let mut command = if executable && !is_app_bundle {
        let mut command = Command::new(path);
        if let Some(dir) = path.parent() {
            command.current_dir(dir);
        }
        command
    } else {
        // App bundles, documents and launchers go through the desktop.
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let mut command = Command::new(opener);
        command.arg(path);
        command
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", path.display()))?;
    // Reap the process when it exits so it does not linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(not(any(windows, unix)))]
fn launch(path: &Path) -> Result<(), String> {
    Err(format!(
        "starting {} is not supported on this operating system",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_programs_are_reported() {
        let err = launch_program("/definitely/not/here/program").unwrap_err();
        assert!(err.contains("not found"));
    }

    #[cfg(unix)]
    #[test]
    fn starts_an_executable_in_its_own_folder() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("start.sh");
        std::fs::write(&script, "#!/bin/sh\npwd > started.txt\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        launch_program(script.to_str().unwrap()).unwrap();
        let marker = dir.path().join("started.txt");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let written = std::fs::read_to_string(&marker).expect("the program did not run");
        assert_eq!(
            std::fs::canonicalize(written.trim()).unwrap(),
            std::fs::canonicalize(dir.path()).unwrap()
        );
    }
}
