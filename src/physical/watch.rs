
use super::manifest::{self, DiscManifest, ManifestError};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeKind {
    Optical,
    Removable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    pub root: PathBuf,
    pub kind: VolumeKind,
    pub label: Option<String>,
}

#[derive(Debug)]
pub struct DiscEvent {
    pub volume: Volume,
    pub manifests: Vec<(PathBuf, Result<DiscManifest, ManifestError>)>,
}

pub struct WatchOptions {
    pub scan_existing: bool,
    pub removable: Arc<AtomicBool>,
    pub interval: Duration,
}

impl Default for WatchOptions {
    fn default() -> Self {
        WatchOptions { scan_existing: true, removable: Arc::new(AtomicBool::new(true)), interval: Duration::from_secs(2) }
    }
}

pub struct Watcher {
    stop: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Watcher {
    pub fn spawn(opts: WatchOptions, mut on_event: impl FnMut(DiscEvent) + Send + 'static) -> Watcher {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::Builder::new()
            .name("yasaff-disc-watch".into())
            .spawn(move || {
                let mut platform = platform::Platform::new();
                let mut seen: HashSet<PathBuf> = HashSet::new();
                if !opts.scan_existing {
                    seen.extend(platform.volumes(true).into_iter().map(|v| v.root));
                }
                while !stopped.load(Ordering::Relaxed) {
                    let volumes = platform.volumes(opts.removable.load(Ordering::Relaxed));
                    seen.retain(|root| volumes.iter().any(|v| &v.root == root));
                    for volume in volumes {
                        if seen.contains(&volume.root) {
                            continue;
                        }
                        let Ok(manifests) = manifest::read_all(&volume.root) else { continue };
                        seen.insert(volume.root.clone());
                        if !manifests.is_empty() {
                            on_event(DiscEvent { volume, manifests });
                        }
                    }
                    sleep_unless_stopped(opts.interval, &stopped);
                }
            })
            .expect("spawning the disc watcher thread");
        Watcher { stop }
    }
}

fn sleep_unless_stopped(total: Duration, stop: &AtomicBool) {
    let step = Duration::from_millis(100);
    let mut slept = Duration::ZERO;
    while slept < total && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(step);
        slept += step;
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{Volume, VolumeKind};
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const AUTOMOUNT_GRACE: Duration = Duration::from_secs(4);

    pub struct Platform {
        unmounted: HashMap<String, (Instant, bool)>,
    }

    struct Mount {
        source: String,
        point: PathBuf,
    }

    impl Platform {
        pub fn new() -> Self {
            Platform { unmounted: HashMap::new() }
        }

        pub fn volumes(&mut self, removable: bool) -> Vec<Volume> {
            let mounts = mounts();
            self.mount_waiting_discs(&mounts);
            mounts
                .into_iter()
                .filter_map(|m| {
                    let kind = if m.source.starts_with("/dev/sr") {
                        VolumeKind::Optical
                    } else if removable && is_removable(&m) {
                        VolumeKind::Removable
                    } else {
                        return None;
                    };
                    let label = m.point.file_name().map(|n| n.to_string_lossy().into_owned());
                    Some(Volume { root: m.point, kind, label })
                })
                .collect()
        }

        fn mount_waiting_discs(&mut self, mounts: &[Mount]) {
            let Ok(entries) = fs::read_dir("/sys/block") else { return };
            let mut present = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with("sr") {
                    continue;
                }
                let device = format!("/dev/{name}");
                let has_media = fs::read_to_string(entry.path().join("size"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .is_some_and(|size| size > 0);
                if has_media && !mounts.iter().any(|m| m.source == device) {
                    present.push(device);
                }
            }
            self.unmounted.retain(|device, _| present.contains(device));
            for device in present {
                let (since, tried) = self.unmounted.entry(device.clone()).or_insert((Instant::now(), false));
                if !*tried && since.elapsed() >= AUTOMOUNT_GRACE {
                    *tried = true;
                    let _ = Command::new("udisksctl")
                        .args(["mount", "--no-user-interaction", "-b", &device])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
            }
        }
    }

    fn mounts() -> Vec<Mount> {
        let Ok(text) = fs::read_to_string("/proc/self/mountinfo") else { return Vec::new() };
        text.lines().filter_map(parse_mountinfo_line).collect()
    }

    fn parse_mountinfo_line(line: &str) -> Option<Mount> {
        let (left, right) = line.split_once(" - ")?;
        let point = left.split(' ').nth(4)?;
        let source = right.split(' ').nth(1)?;
        Some(Mount { source: unescape(source), point: PathBuf::from(unescape(point)) })
    }

    fn unescape(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let octal = bytes.get(i + 1..i + 4).filter(|d| bytes[i] == b'\\' && d.iter().all(|b| (b'0'..=b'7').contains(b)));
            match octal {
                Some(d) => {
                    out.push((d[0] - b'0') * 64 + (d[1] - b'0') * 8 + (d[2] - b'0'));
                    i += 4;
                }
                None => {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn is_removable(m: &Mount) -> bool {
        if !m.source.starts_with("/dev/") {
            return false;
        }
        if m.point.starts_with("/run/media") || m.point.starts_with("/media") {
            return true;
        }
        let name = m.source.trim_start_matches("/dev/");
        let Ok(sys) = fs::canonicalize(Path::new("/sys/class/block").join(name)) else { return false };
        [sys.join("removable"), sys.parent().map(|p| p.join("removable")).unwrap_or_default()]
            .iter()
            .any(|f| fs::read_to_string(f).is_ok_and(|v| v.trim() == "1"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn mountinfo_lines() {
            let m = parse_mountinfo_line(
                "812 30 11:0 / /run/media/me/MY\\040GAME ro,nosuid shared:442 - iso9660 /dev/sr0 ro,norock",
            )
            .unwrap();
            assert_eq!(m.source, "/dev/sr0");
            assert_eq!(m.point, PathBuf::from("/run/media/me/MY GAME"));
            assert!(parse_mountinfo_line("garbage").is_none());
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::{Volume, VolumeKind};
    use std::path::PathBuf;
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
    use windows_sys::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetThreadErrorMode};

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_CDROM: u32 = 5;

    pub struct Platform;

    impl Platform {
        pub fn new() -> Self {
            unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, std::ptr::null_mut()) };
            Platform
        }

        pub fn volumes(&mut self, removable: bool) -> Vec<Volume> {
            let mask = unsafe { GetLogicalDrives() };
            (2..26u8)
                .filter(|i| mask & (1 << i) != 0)
                .filter_map(|i| {
                    let root = format!("{}:\\", (b'A' + i) as char);
                    let wide: Vec<u16> = root.encode_utf16().chain([0]).collect();
                    let kind = match unsafe { GetDriveTypeW(wide.as_ptr()) } {
                        DRIVE_CDROM => VolumeKind::Optical,
                        DRIVE_REMOVABLE if removable => VolumeKind::Removable,
                        _ => return None,
                    };
                    let mut label = [0u16; 261];
                    let ok = unsafe {
                        GetVolumeInformationW(
                            wide.as_ptr(),
                            label.as_mut_ptr(),
                            label.len() as u32,
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            0,
                        )
                    };
                    if ok == 0 {
                        return None;
                    }
                    let len = label.iter().position(|&c| c == 0).unwrap_or(label.len());
                    let label = String::from_utf16_lossy(&label[..len]);
                    Some(Volume { root: PathBuf::from(root), kind, label: (!label.is_empty()).then_some(label) })
                })
                .collect()
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    use super::Volume;

    pub struct Platform;

    impl Platform {
        pub fn new() -> Self {
            Platform
        }

        pub fn volumes(&mut self, _removable: bool) -> Vec<Volume> {
            Vec::new()
        }
    }
}
