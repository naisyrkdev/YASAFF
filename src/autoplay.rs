//! Keeps the system's file manager from popping up a window when a USB stick, SD card or disc is
//! inserted while YASAFF runs, so a game started from it opens without a folder in front.
//!
//! The desktop's own settings are switched off for as long as a [`Suppression`] lives and put back
//! when it drops. What was there before is also written to a file first, so a crash is undone the
//! next time YASAFF starts ([`recover`]).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One system setting YASAFF changed and the value it had before.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Saved {
    tool: Tool,
    /// Schema/channel and key, as the tool names them.
    place: (String, String),
    /// `None` when the value was not set before (Windows registry only).
    original: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Gsettings,
    Xfconf,
    Registry,
}

fn restore_file() -> PathBuf {
    crate::profiles::root().join("autoplay-restore.json")
}

/// The system settings switched off while it lives.
pub struct Suppression {
    saved: Vec<Saved>,
}

impl Drop for Suppression {
    fn drop(&mut self) {
        restore(&self.saved);
        let _ = std::fs::remove_file(restore_file());
    }
}

/// Switches off the file manager windows that open on insert. `None` when there was nothing to
/// switch off on this system.
pub fn suppress() -> Option<Suppression> {
    let saved = imp::suppress();
    if saved.is_empty() {
        return None;
    }
    if let Ok(json) = serde_json::to_vec_pretty(&saved) {
        if let Err(e) = crate::fsutil::write_atomic(&restore_file(), json) {
            log::warn!("could not record the autoplay settings to restore: {e:#}");
        }
    }
    Some(Suppression { saved })
}

/// Puts back settings a previous run switched off but never restored (it crashed or was killed).
pub fn recover() {
    let path = restore_file();
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    match serde_json::from_str::<Vec<Saved>>(&text) {
        Ok(saved) => {
            log::info!("restoring {} autoplay setting(s) left from a previous run", saved.len());
            restore(&saved);
        }
        Err(e) => log::warn!("ignoring {}: {e}", path.display()),
    }
    let _ = std::fs::remove_file(path);
}

fn restore(saved: &[Saved]) {
    for s in saved {
        if let Err(e) = imp::restore(s) {
            log::warn!("could not restore {:?} {}/{}: {e:#}", s.tool, s.place.0, s.place.1);
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::{Saved, Tool};
    use anyhow::{Context, Result, bail};
    use std::process::{Command, Stdio};

    /// `(schema, key, value while YASAFF runs)` for the desktops that open a folder on mount.
    /// `automount-open` is the folder window; `autorun-never` is the "what to do with this
    /// media" prompt. Mounting itself stays on: YASAFF needs the stick mounted to read it.
    const GSETTINGS: &[(&str, &str, &str)] = &[
        ("org.gnome.desktop.media-handling", "automount-open", "false"),
        ("org.gnome.desktop.media-handling", "autorun-never", "true"),
        ("org.cinnamon.desktop.media-handling", "automount-open", "false"),
        ("org.cinnamon.desktop.media-handling", "autorun-never", "true"),
        ("org.mate.media-handling", "automount-open", "false"),
        ("org.mate.media-handling", "autorun-never", "true"),
    ];

    /// Thunar's volume manager (XFCE): opens a Thunar window on mount.
    const XFCONF: &[(&str, &str, &str)] = &[("thunar-volman", "/autobrowse/enabled", "false")];

    fn run(program: &str, args: &[&str]) -> Result<String> {
        let out = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .with_context(|| format!("running {program}"))?;
        if !out.status.success() {
            bail!("{program} {} failed", args.join(" "));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    fn schema_installed(schema: &str) -> bool {
        run("gsettings", &["list-schemas"]).is_ok_and(|list| list.lines().any(|l| l == schema))
    }

    pub fn suppress() -> Vec<Saved> {
        let mut saved = Vec::new();
        for &(schema, key, want) in GSETTINGS {
            if !schema_installed(schema) {
                continue;
            }
            let Ok(original) = run("gsettings", &["get", schema, key]) else { continue };
            if original == want {
                continue;
            }
            if run("gsettings", &["set", schema, key, want]).is_ok() {
                saved.push(Saved { tool: Tool::Gsettings, place: (schema.into(), key.into()), original: Some(original) });
            }
        }
        for &(channel, property, want) in XFCONF {
            let Ok(original) = run("xfconf-query", &["-c", channel, "-p", property]) else { continue };
            if original == want {
                continue;
            }
            if run("xfconf-query", &["-c", channel, "-p", property, "-s", want]).is_ok() {
                saved.push(Saved { tool: Tool::Xfconf, place: (channel.into(), property.into()), original: Some(original) });
            }
        }
        saved
    }

    pub fn restore(s: &Saved) -> Result<()> {
        let (a, b) = (s.place.0.as_str(), s.place.1.as_str());
        let Some(value) = s.original.as_deref() else { return Ok(()) };
        match s.tool {
            Tool::Gsettings => run("gsettings", &["set", a, b, value]).map(drop),
            Tool::Xfconf => run("xfconf-query", &["-c", a, "-p", b, "-s", value]).map(drop),
            Tool::Registry => Ok(()),
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::{Saved, Tool};
    use anyhow::{Result, bail};
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW};

    /// The "Use AutoPlay for all media and devices" switch in Settings › Bluetooth & devices ›
    /// AutoPlay. Explorer reads it on every insert, so it applies right away.
    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\AutoplayHandlers";
    const VALUE: &str = "DisableAutoplay";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn get(key: &str, value: &str) -> Option<u32> {
        let (key, value) = (wide(key), wide(value));
        let mut data = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut data as *mut u32).cast(),
                &mut size,
            )
        };
        (status == ERROR_SUCCESS).then_some(data)
    }

    fn set(key: &str, value: &str, data: Option<u32>) -> Result<()> {
        let (key, value) = (wide(key), wide(value));
        let status = match data {
            Some(data) => unsafe {
                RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), REG_DWORD, (&data as *const u32).cast(), 4)
            },
            None => match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) } {
                ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
                status => status,
            },
        };
        if status != ERROR_SUCCESS {
            bail!("{}", std::io::Error::from_raw_os_error(status as i32));
        }
        Ok(())
    }

    pub fn suppress() -> Vec<Saved> {
        let original = get(KEY, VALUE);
        if original == Some(1) {
            return Vec::new();
        }
        match set(KEY, VALUE, Some(1)) {
            Ok(()) => vec![Saved { tool: Tool::Registry, place: (KEY.into(), VALUE.into()), original: original.map(|v| v.to_string()) }],
            Err(e) => {
                log::warn!("could not turn off AutoPlay: {e:#}");
                Vec::new()
            }
        }
    }

    pub fn restore(s: &Saved) -> Result<()> {
        if s.tool != Tool::Registry {
            return Ok(());
        }
        set(&s.place.0, &s.place.1, s.original.as_deref().and_then(|v| v.parse().ok()))
    }
}

#[cfg(not(any(all(unix, not(target_os = "macos")), windows)))]
mod imp {
    use super::Saved;

    pub fn suppress() -> Vec<Saved> {
        Vec::new()
    }

    pub fn restore(_: &Saved) -> anyhow::Result<()> {
        Ok(())
    }
}
