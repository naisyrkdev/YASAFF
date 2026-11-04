//! The portable Windows build carries `portable-storage.txt` next to `yasaff.exe` (written by
//! `installer/build-portable.ps1`); the installer and builds from source never have it, so they
//! keep using the normal per-user system folder and never hit any of this.
//!
//! On the portable build's first run, that file holds `ask`: this module then asks, once, whether
//! to keep everything self-contained next to the exe (so the whole folder can move, e.g. on a USB
//! stick) or to use the normal per-user system folder (reusing whatever YASAFF already has there).
//! The answer is written back into the file so later runs don't ask again.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const MARKER_FILE: &str = "portable-storage.txt";
const PORTABLE_CHOICE: &str = "portable";
const SYSTEM_CHOICE: &str = "system";
const DATA_SUBDIR: &str = "data";

fn exe_dir() -> Option<PathBuf> {
    crate::profiles::exe().and_then(|e| e.parent().map(Path::to_path_buf))
}

fn system_root() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "yasaff").map(|d| d.data_dir().to_path_buf())
}

/// `Some` overrides the normal per-user system folder with a self-contained one; `None` leaves
/// `profiles::root()` to use the system folder, as every non-portable build always does.
pub(crate) fn root_override() -> Option<PathBuf> {
    static CHOICE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CHOICE.get_or_init(decide).clone()
}

fn decide() -> Option<PathBuf> {
    let dir = exe_dir()?;
    let marker = dir.join(MARKER_FILE);
    let choice = match std::fs::read_to_string(&marker) {
        Ok(text) => text.trim().to_owned(),
        Err(_) => return None, // not the portable build
    };
    let portable = match choice.as_str() {
        PORTABLE_CHOICE => true,
        SYSTEM_CHOICE => false,
        _ => {
            let portable = ask();
            let _ = std::fs::write(&marker, if portable { PORTABLE_CHOICE } else { SYSTEM_CHOICE });
            portable
        }
    };
    portable.then(|| dir.join(DATA_SUBDIR))
}

/// A native Yes/No dialog, since this runs before the main window (and before settings, so the
/// user's chosen language isn't known yet) exists.
fn ask() -> bool {
    let found_existing = system_root().is_some_and(|d| d.join("profiles").is_dir());
    let description = if found_existing {
        "YASAFF found existing library and settings in your Windows user account.\n\n\
        Yes: keep using them there.\n\
        No: copy nothing, and keep this portable copy self-contained in its own folder instead \
        (so it can be moved, e.g. on a USB stick, without leaving anything behind)."
    } else {
        "Where should this portable copy of YASAFF keep your library, settings and saves?\n\n\
        Yes: in your Windows user account, like a normal install.\n\
        No: in this copy's own folder, so it stays fully self-contained (for example on a USB stick)."
    };
    rfd::MessageDialog::new()
        .set_title("YASAFF")
        .set_description(description)
        .set_level(rfd::MessageLevel::Info)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::No
}
