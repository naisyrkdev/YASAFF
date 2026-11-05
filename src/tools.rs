use std::path::PathBuf;

// External programs (ffmpeg, xorriso) ship in a `tools` folder: next to yasaff.exe from the
// Windows installer, and at the root of the AppImage ($APPDIR/tools). Builds from source fall
// back to whatever is on PATH.
fn bundled_dirs() -> Vec<PathBuf> {
    let appdir = std::env::var_os("APPDIR").map(|d| PathBuf::from(d).join("tools"));
    let exe = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("tools")));
    appdir.into_iter().chain(exe).collect()
}

// An app started from Finder or the Dock gets launchd's PATH (/usr/bin:/bin:/usr/sbin:/sbin),
// which leaves out where Homebrew and MacPorts install ffmpeg.
#[cfg(target_os = "macos")]
const EXTRA_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"];
#[cfg(not(target_os = "macos"))]
const EXTRA_DIRS: &[&str] = &[];

fn file_name(program: &str) -> String {
    format!("{program}{}", std::env::consts::EXE_SUFFIX)
}

fn bundled(program: &str) -> Option<PathBuf> {
    let name = file_name(program);
    bundled_dirs().into_iter().map(|dir| dir.join(&name)).find(|p| p.is_file())
}

fn on_path(program: &str) -> bool {
    let name = file_name(program);
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(&name).is_file()))
}

fn in_extra_dirs(program: &str) -> Option<PathBuf> {
    let name = file_name(program);
    EXTRA_DIRS.iter().map(|dir| PathBuf::from(dir).join(&name)).find(|p| p.is_file())
}

/// What to hand to `Command::new`: the bundled copy if there is one, otherwise the bare name.
pub fn program(program: &str) -> PathBuf {
    bundled(program)
        .or_else(|| if on_path(program) { None } else { in_extra_dirs(program) })
        .unwrap_or_else(|| PathBuf::from(program))
}

pub fn installed(program: &str) -> bool {
    bundled(program).is_some() || on_path(program) || in_extra_dirs(program).is_some()
}
