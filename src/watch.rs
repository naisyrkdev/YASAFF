//! Follows a launched game so YASAFF knows when it is up, can close it, and can come back when it ends.
//!
//! When YASAFF started the game itself it knows the root process and follows that process tree
//! (`Tracking::Tree`). When the game goes through a store or a URI handler the real game process
//! is started by someone else, so it is found by looking for a big, new process whose ancestry
//! leads to a known launcher (`Tracking::Search`).

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System};

/// A process this big is past its launcher/splash stage and is taken to be the game itself.
const GAME_MEMORY: u64 = 300 * 1024 * 1024;
const POLL: Duration = Duration::from_millis(1000);
const FIND_TIMEOUT: Duration = Duration::from_secs(300);
/// A launched process tree that is gone this soon, without anything game-sized in it, was a wrapper.
const WRAPPER_GRACE: Duration = Duration::from_secs(15);

/// Processes that are never the game, even when they are big.
const HELPERS: &[&str] = &[
    "steam", "steamwebhelper", "steam-runtime", "pressure-vessel", "pv-bwrap", "srt-bwrap", "reaper",
    "heroic", "gogdl", "legendary", "nile", "comet", "wineserver", "wine", "wine64", "wine-preloader",
    "wine64-preloader", "winedevice.exe", "services.exe", "explorer.exe", "plugplay.exe", "rpcss.exe",
    "tabtip.exe", "conhost.exe", "xdg-open", "flatpak", "bwrap", "umu-run", "python3", "python",
    "steam.exe", "steamservice.exe", "heroic.exe", "epicgameslauncher.exe", "galaxyclient.exe",
    "lutris", "lutris-wrapper",
];

/// Ancestors that prove a process was started by a game launcher. Desktop shells (`explorer.exe`,
/// `systemd --user`) are deliberately absent: everything the user opens descends from them.
const LAUNCHERS: &[&str] = &[
    "steam", "steam-runtime", "pressure-vessel", "pv-bwrap", "srt-bwrap", "reaper", "heroic", "gogdl",
    "legendary", "nile", "comet", "wine", "wine64", "wine-preloader", "wine64-preloader", "wineserver",
    "xdg-open", "flatpak", "umu-run", "steam.exe", "steamservice.exe", "heroic.exe",
    "epicgameslauncher.exe", "galaxyclient.exe", "lutris", "lutris-wrapper",
];

fn named(p: &Process, names: &[&str]) -> bool {
    let name = p.name().to_string_lossy().to_lowercase();
    names.iter().any(|h| name == *h || name.starts_with(&format!("{h}-")))
}

/// Whether `p` descends from YASAFF or from a game launcher, without passing a process that is
/// older than the launch (which would make it something the user already had open).
///
/// An orphan (its ancestry ends at init or the session's `systemd`) only counts when
/// `orphans_until` is set and it started before then: that is a game whose wrapper script exited.
fn launched_by_us(sys: &System, p: &Process, since: u64, own_pid: u32, orphans_until: Option<u64>) -> bool {
    let orphan = || orphans_until.is_some_and(|until| p.start_time() <= until);
    let mut parent = p.parent();
    for _ in 0..64 {
        let Some(pid) = parent else { return orphan() };
        if pid.as_u32() == own_pid {
            return true;
        }
        let Some(proc) = sys.process(pid) else { return orphan() };
        if named(proc, LAUNCHERS) {
            return true;
        }
        if pid.as_u32() <= 1 || proc.name() == "systemd" {
            return orphan();
        }
        if proc.start_time() + 1 < since {
            return false;
        }
        parent = proc.parent();
    }
    false
}

#[derive(Clone)]
pub struct GameWatch(Arc<Shared>);

struct Shared {
    /// The game-sized process, once one appeared (0 until then).
    pid: AtomicU32,
    /// Every live process that belongs to the game; empty in `Search` mode until the game is found.
    tree: Mutex<HashSet<u32>>,
    tracked: AtomicBool,
    ended: AtomicBool,
    cancel: AtomicBool,
}

impl Shared {
    fn seen(&self) -> bool {
        self.pid.load(Ordering::Relaxed) != 0
    }
}

impl GameWatch {
    /// `root` is the process YASAFF spawned, when it spawned one; without it the game is searched for.
    pub fn start(ctx: egui::Context, root: Option<u32>) -> Self {
        let shared = Arc::new(Shared {
            pid: AtomicU32::new(0),
            tree: Mutex::new(root.into_iter().collect()),
            tracked: AtomicBool::new(root.is_some()),
            ended: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
        });
        let watch = GameWatch(shared.clone());
        let spawned = std::thread::Builder::new().name("game-watch".into()).spawn(move || {
            let since = now_secs();
            let started = Instant::now();
            if let Some(root) = root {
                follow_tree(Pid::from_u32(root), &shared, &ctx);
            }
            // A wrapper script that starts the game in the background and exits leaves the tree
            // before the game shows up; look for the game the way store launches do.
            let wrapper_exited = root.is_some() && started.elapsed() < WRAPPER_GRACE;
            if !shared.seen() && (root.is_none() || wrapper_exited) && !shared.cancel.load(Ordering::Relaxed) {
                shared.tracked.store(false, Ordering::Relaxed);
                let orphans_until = wrapper_exited.then(|| now_secs() + 1);
                if let Some(pid) = find_game(&shared.cancel, since, orphans_until, FIND_TIMEOUT.saturating_sub(started.elapsed())) {
                    shared.pid.store(pid.as_u32(), Ordering::Relaxed);
                    shared.tree.lock().unwrap_or_else(|e| e.into_inner()).insert(pid.as_u32());
                    shared.tracked.store(true, Ordering::Relaxed);
                    ctx.request_repaint();
                    follow_tree(pid, &shared, &ctx);
                }
            }
            shared.ended.store(true, Ordering::Relaxed);
            ctx.request_repaint();
        });
        if let Err(e) = spawned {
            log::error!("can't watch the game: {e}");
            watch.0.ended.store(true, Ordering::Relaxed);
        }
        watch
    }

    /// A game-sized process is up, so the game has most likely opened its window.
    pub fn seen(&self) -> bool {
        self.0.seen()
    }

    /// YASAFF knows which processes are the game, so it can close it and notice when it ends.
    pub fn tracked(&self) -> bool {
        self.0.tracked.load(Ordering::Relaxed)
    }

    pub fn ended(&self) -> bool {
        self.0.ended.load(Ordering::Relaxed)
    }

    pub fn stop(&self) {
        self.0.cancel.store(true, Ordering::Relaxed);
    }

    /// Kills every process of the game, children first. False when there was nothing to kill.
    pub fn kill(&self) -> bool {
        let pids: Vec<Pid> = self.0.tree.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|p| Pid::from_u32(*p)).collect();
        if pids.is_empty() {
            return false;
        }
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::Some(&pids), true);
        let mut alive: Vec<&Process> = pids.iter().filter_map(|p| sys.process(*p)).collect();
        alive.sort_by_key(|p| std::cmp::Reverse(p.start_time()));
        let mut killed = false;
        for p in alive {
            killed |= p.kill();
        }
        killed
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn find_game(cancel: &AtomicBool, since: u64, orphans_until: Option<u64>, timeout: Duration) -> Option<Pid> {
    let started = Instant::now();
    let own_pid = std::process::id();
    let mut sys = System::new();
    let kind = ProcessRefreshKind::nothing().with_memory();
    while !cancel.load(Ordering::Relaxed) && started.elapsed() < timeout {
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        let game = sys.processes().values().find(|p| {
            p.thread_kind().is_none()
                && p.pid().as_u32() != own_pid
                && p.start_time() + 1 >= since
                && p.memory() >= GAME_MEMORY
                && !named(p, HELPERS)
                && launched_by_us(&sys, p, since, own_pid, orphans_until)
        });
        if let Some(p) = game {
            log::info!("game process detected: {} ({} MiB)", p.name().to_string_lossy(), p.memory() >> 20);
            return Some(p.pid());
        }
        std::thread::sleep(POLL);
    }
    None
}

/// Follows `root` and everything it starts until all of them have exited. Children are collected
/// while their parent is alive, so they stay followed after the parent exits and they are reparented.
fn follow_tree(root: Pid, shared: &Shared, ctx: &egui::Context) {
    let mut sys = System::new();
    let kind = ProcessRefreshKind::nothing().with_memory();
    while !shared.cancel.load(Ordering::Relaxed) {
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        let mut tree = shared.tree.lock().unwrap_or_else(|e| e.into_inner());
        let mut grew = true;
        while grew {
            grew = false;
            for (pid, p) in sys.processes() {
                if p.thread_kind().is_none() && !tree.contains(&pid.as_u32()) && p.parent().is_some_and(|parent| tree.contains(&parent.as_u32())) {
                    tree.insert(pid.as_u32());
                    grew = true;
                }
            }
        }
        tree.retain(|pid| sys.process(Pid::from_u32(*pid)).is_some_and(|p| p.status() != sysinfo::ProcessStatus::Zombie));
        if tree.is_empty() {
            log::info!("game process {root} and its children exited");
            return;
        }
        if shared.pid.load(Ordering::Relaxed) == 0
            && let Some(big) = tree.iter().find(|pid| sys.process(Pid::from_u32(**pid)).is_some_and(|p| p.memory() >= GAME_MEMORY && !named(p, HELPERS)))
        {
            shared.pid.store(*big, Ordering::Relaxed);
            ctx.request_repaint();
        }
        drop(tree);
        std::thread::sleep(POLL);
    }
}
