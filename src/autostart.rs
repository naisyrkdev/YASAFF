use anyhow::{Context, Result};
use std::path::PathBuf;

fn program() -> Result<PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Ok(appimage.into());
    }
    std::env::current_exe().context("could not locate the YASAFF executable")
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::*;
    use std::fs;

    fn entry() -> Option<PathBuf> {
        let dirs = directories::BaseDirs::new()?;
        Some(dirs.config_dir().join("autostart").join("yasaff.desktop"))
    }

    pub fn is_enabled() -> bool {
        entry().is_some_and(|p| fs::read_to_string(p).is_ok_and(|text| !text.lines().any(|l| l.trim() == "Hidden=true")))
    }

    pub fn set(enabled: bool) -> Result<()> {
        let path = entry().context("could not find the config directory")?;
        if !enabled {
            return match fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).context(format!("removing {}", path.display())),
                _ => Ok(()),
            };
        }
        let program = program()?;
        let mut exec = String::from('"');
        for c in program.to_string_lossy().chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                exec.push('\\');
            }
            exec.push(c);
        }
        exec.push('"');
        let desktop = format!(
            "[Desktop Entry]\nType=Application\nName=YASAFF\nComment=Game launcher\nExec={exec}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
        );
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        fs::write(&path, desktop).with_context(|| format!("writing {}", path.display()))
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW};

    fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
        s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
    }

    fn key() -> Vec<u16> {
        wide(r"Software\Microsoft\Windows\CurrentVersion\Run")
    }

    fn value() -> Vec<u16> {
        wide("YASAFF")
    }

    pub fn is_enabled() -> bool {
        let (key, value) = (key(), value());
        let status = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut())
        };
        status == ERROR_SUCCESS
    }

    pub fn set(enabled: bool) -> Result<()> {
        let (key, value) = (key(), value());
        let status = if enabled {
            let command = wide(format!("\"{}\"", program()?.display()));
            unsafe {
                RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), REG_SZ, command.as_ptr().cast(), (command.len() * 2) as u32)
            }
        } else {
            match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) } {
                ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
                status => status,
            }
        };
        if status != ERROR_SUCCESS {
            anyhow::bail!("{}", std::io::Error::from_raw_os_error(status as i32));
        }
        Ok(())
    }
}

#[cfg(not(any(all(unix, not(target_os = "macos")), windows)))]
mod imp {
    use super::*;

    pub fn is_enabled() -> bool {
        false
    }

    pub fn set(_enabled: bool) -> Result<()> {
        let _ = program;
        anyhow::bail!("not supported on this system")
    }
}

pub use imp::{is_enabled, set};
