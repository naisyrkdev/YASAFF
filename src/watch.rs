
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System};

const GAME_MEMORY: u64 = 300 * 1024 * 1024;
const POLL: Duration = Duration::from_millis(1000);
const FIND_TIMEOUT: Duration = Duration::from_secs(300);

const HELPERS: &[&str] = &[
    "steam", "steamwebhelper", "steam-runtime", "pressure-vessel", "pv-bwrap", "srt-bwrap", "reaper",
    "heroic", "gogdl", "legendary", "nile", "comet", "wineserver", "wine", "wine64", "wine-preloader",
    "wine64-preloader", "winedevice.exe", "services.exe", "explorer.exe", "plugplay.exe", "rpcss.exe",
    "tabtip.exe", "conhost.exe", "xdg-open", "flatpak", "bwrap", "umu-run", "python3", "python",
    "steam.exe", "steamservice.exe", "heroic.exe", "epicgameslauncher.exe", "galaxyclient.exe",
];

fn is_helper(p: &Process) -> bool {
    let name = p.name().to_string_lossy().to_lowercase();
    HELPERS.iter().any(|h| name == *h || name.starts_with(&format!("{h}-")))
}

fn launched_by_us(sys: &System, p: &Process, since: u64, own_pid: u32) -> bool {
    let mut parent = p.parent();
    for _ in 0..64 {
        let Some(pid) = parent else { return true };
        let Some(proc) = sys.process(pid) else { return true };
        if pid.as_u32() == own_pid || is_helper(proc) {
            return true;
        }
        if is_root(pid, proc) {
            return true;
        }
        if proc.start_time() + 1 < since {
            return false;
        }
        parent = proc.parent();
    }
    false
}

fn is_root(pid: Pid, p: &Process) -> bool {
    pid.as_u32() <= 1 || p.name() == "systemd"
}

#[derive(Clone)]
pub struct GameWatch(Arc<Shared>);

struct Shared {
    pid: AtomicU32,
    ended: AtomicBool,
    cancel: AtomicBool,
}

impl GameWatch {
    pub fn start(ctx: egui::Context) -> Self {
        let watch = GameWatch(Arc::new(Shared { pid: AtomicU32::new(0), ended: AtomicBool::new(false), cancel: AtomicBool::new(false) }));
        let shared = watch.0.clone();
        std::thread::spawn(move || {
            let found = find_game(&shared.cancel);
            if let Some(pid) = found {
                shared.pid.store(pid.as_u32(), Ordering::Relaxed);
                ctx.request_repaint();
                follow(pid, &shared.cancel);
            }
            shared.ended.store(true, Ordering::Relaxed);
            ctx.request_repaint();
        });
        watch
    }

    pub fn seen(&self) -> bool {
        self.0.pid.load(Ordering::Relaxed) != 0
    }

    pub fn ended(&self) -> bool {
        self.0.ended.load(Ordering::Relaxed)
    }

    pub fn stop(&self) {
        self.0.cancel.store(true, Ordering::Relaxed);
    }

    pub fn kill(&self) -> bool {
        let pid = Pid::from_u32(self.0.pid.load(Ordering::Relaxed));
        if pid.as_u32() == 0 {
            return false;
        }
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        sys.process(pid).is_some_and(Process::kill)
    }
}

fn find_game(cancel: &AtomicBool) -> Option<Pid> {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let started = Instant::now();
    let own_pid = std::process::id();
    let mut sys = System::new();
    let kind = ProcessRefreshKind::nothing().with_memory();
    while !cancel.load(Ordering::Relaxed) && started.elapsed() < FIND_TIMEOUT {
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        let game = sys.processes().values().find(|p| {
            p.thread_kind().is_none()
                && p.pid().as_u32() != own_pid
                && p.start_time() + 1 >= since
                && p.memory() >= GAME_MEMORY
                && !is_helper(p)
                && launched_by_us(&sys, p, since, own_pid)
        });
        if let Some(p) = game {
            eprintln!("game process detected: {} ({} MiB)", p.name().to_string_lossy(), p.memory() >> 20);
            return Some(p.pid());
        }
        std::thread::sleep(POLL);
    }
    None
}

fn follow(pid: Pid, cancel: &AtomicBool) {
    let mut sys = System::new();
    while !cancel.load(Ordering::Relaxed) {
        sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing());
        if sys.process(pid).is_none_or(|p| p.status() == sysinfo::ProcessStatus::Zombie) {
            eprintln!("game process {pid} exited");
            return;
        }
        std::thread::sleep(POLL);
    }
}
