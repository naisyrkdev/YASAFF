//! Steam games owned but not installed: starting their install through Steam and following it.
//! A background thread reads each Steam game's `appmanifest_<appid>.acf`, so installs and
//! uninstalls done in Steam itself (or while YASAFF was closed) show up too: a game Steam finished
//! installing joins the home screen, and one uninstalled in Steam moves to the Library tile.

use super::*;
use crate::steam::{self, Install};
use std::collections::HashMap;

/// How often games that aren't installed are checked; installed ones are checked
/// `INSTALLED_EVERY` times less often, since they rarely change.
const TICK: Duration = Duration::from_secs(3);
const INSTALLED_EVERY: u32 = 10;
/// How often the list of Steam games in the library is handed to the watcher again.
const RESYNC: Duration = Duration::from_secs(2);

/// One library game the watcher follows: its id, its Steam appid and whether the library thinks
/// it is installed.
#[derive(Clone, PartialEq)]
struct Watched {
    game_id: String,
    appid: String,
    installed: bool,
}

pub(super) struct Installs {
    /// The account last logged into Steam here; read once, and again when Settings asks.
    pub(super) steam_id: Option<String>,
    /// The account signed in with the QR code, by name, for Settings.
    pub(super) signed_in: Option<String>,
    /// Games Steam is downloading now, by game id: how far along, when Steam knows.
    pub(super) progress: HashMap<String, Option<f32>>,
    watch: mpsc::Sender<Vec<Watched>>,
    updates: mpsc::Receiver<(String, Install)>,
    sent: Vec<Watched>,
    synced: Option<Instant>,
}

impl Installs {
    pub(super) fn new(ctx: Context) -> Self {
        let (watch, watch_rx) = mpsc::channel::<Vec<Watched>>();
        let (updates_tx, updates) = mpsc::channel();
        std::thread::spawn(move || watcher(ctx, watch_rx, updates_tx));
        let signed_in = steam::Session::load().map(|s| if s.account_name.is_empty() { s.steam_id } else { s.account_name });
        Installs { steam_id: steam::detect_steam_id(), signed_in, progress: HashMap::new(), watch, updates, sent: Vec::new(), synced: None }
    }

    pub(super) fn refresh_steam_id(&mut self) {
        self.steam_id = steam::detect_steam_id();
    }
}

fn watcher(ctx: Context, list: mpsc::Receiver<Vec<Watched>>, updates: mpsc::Sender<(String, Install)>) {
    let mut games: Vec<Watched> = Vec::new();
    let mut known: HashMap<String, Install> = HashMap::new();
    let mut tick: u32 = 0;
    loop {
        match list.recv_timeout(TICK) {
            Ok(newest) => {
                games = newest;
                // Take only the latest list if several queued up.
                while let Ok(newest) = list.try_recv() {
                    games = newest;
                }
                known.retain(|id, _| games.iter().any(|g| g.game_id == *id));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        tick = tick.wrapping_add(1);
        // No Steam library readable (Steam missing, drive not mounted): change nothing.
        let Some(libraries) = steam::libraries() else { continue };
        let mut changed = false;
        for game in &games {
            if game.installed && tick % INSTALLED_EVERY != 1 && known.contains_key(&game.game_id) {
                continue;
            }
            let status = steam::install_status(&libraries, &game.appid);
            let expected = if game.installed { Install::Installed } else { Install::NotInstalled };
            let previous = known.insert(game.game_id.clone(), status);
            if previous.map_or(status != expected, |p| p != status) {
                if updates.send((game.game_id.clone(), status)).is_err() {
                    return;
                }
                changed = true;
            }
        }
        if changed {
            ctx.request_repaint();
        }
    }
}

impl App {
    /// The appid of a Steam game in the library, `None` for anything else.
    pub(super) fn steam_appid(game: &Game) -> Option<String> {
        if game.app_id.is_some() {
            return None;
        }
        steam::appid_of(&game.target, &game.args)
    }

    /// Hands the watcher the library's Steam games when they changed, and applies what it found.
    pub(super) fn poll_installs(&mut self) {
        if self.installs.synced.is_none_or(|at| at.elapsed() >= RESYNC) {
            self.installs.synced = Some(Instant::now());
            let watched = |games: &[Game], installed: bool| -> Vec<Watched> {
                games.iter().filter_map(|g| Some(Watched { game_id: g.id.clone(), appid: Self::steam_appid(g)?, installed })).collect()
            };
            let mut list = watched(&self.library.games, true);
            list.extend(watched(&self.library.uninstalled, false));
            if list != self.installs.sent {
                let _ = self.installs.watch.send(list.clone());
                self.installs.sent = list;
            }
        }
        let mut save = false;
        while let Ok((game_id, status)) = self.installs.updates.try_recv() {
            let waiting = self.library.uninstalled.iter().any(|g| g.id == game_id);
            match status {
                Install::Installed => {
                    self.installs.progress.remove(&game_id);
                    if waiting {
                        let selected = self.selected_game().map(|g| g.id.clone());
                        let Some(i) = self.library.move_to_installed(&game_id) else { continue };
                        self.reselect(selected);
                        save = true;
                        let (name, has_cover) = (self.library.games[i].name.clone(), self.library.games[i].has_cover());
                        self.notify(tr!("{name} is installed and ready to play", name = name));
                        if !has_cover {
                            self.fetch_artwork(std::slice::from_ref(&game_id));
                        }
                    }
                }
                Install::Downloading(progress) if waiting => {
                    self.installs.progress.insert(game_id, progress);
                }
                // An installed game Steam is updating also lacks the "fully installed" flag for a
                // while; it stays on the home screen.
                Install::Downloading(_) => {}
                Install::NotInstalled => {
                    self.installs.progress.remove(&game_id);
                    let running = self.game.as_ref().is_some_and(|g| g.game_id == game_id);
                    if let Some(i) = self.library.position(&game_id).filter(|_| !running) {
                        let selected = self.selected_game().map(|g| g.id.clone());
                        self.library.move_to_uninstalled(i);
                        self.reselect(selected);
                        save = true;
                    }
                }
            }
        }
        if save {
            self.installs.sent.clear();
            self.save();
        }
    }

    /// Selects the game with `id` again after games moved, or keeps the selection in range.
    fn reselect(&mut self, id: Option<String>) {
        if let Some(i) = id.and_then(|id| self.library.position(&id)) {
            self.selected = i;
        }
        self.clamp_selection();
    }

    /// Asks Steam to install `appid`; Steam shows its own install dialog. The game waits in
    /// `Library::uninstalled` until Steam has installed it.
    pub(super) fn install_steam_game(&mut self, appid: &str, name: &str) {
        let id = match self.library.uninstalled.iter().find(|g| Self::steam_appid(g).as_deref() == Some(appid)) {
            Some(game) => game.id.clone(),
            None => {
                let id = self.library.add_uninstalled(&scan::game_entry(name.to_owned(), steam::run_uri(appid), None));
                self.save();
                id
            }
        };
        let uri = steam::install_uri(appid);
        let result = match self.settings.store_command(Store::Steam) {
            Some(command) => Store::Steam.launch_uri(&command, &uri),
            None => open::that_detached(&uri).map_err(anyhow::Error::from),
        };
        match result {
            Ok(()) => {
                self.installs.progress.entry(id).or_insert(None);
                self.notify(tr!("Steam is installing {name}. Confirm in Steam's window", name = name));
            }
            Err(e) => self.notify(tr!("Could not ask Steam to install {name}: {error}", name = name, error = format!("{e:#}"))),
        }
    }

    /// How far Steam is with an uninstalled game, as shown in the Library.
    pub(super) fn install_label(&self, game_id: Option<&str>) -> String {
        match game_id.and_then(|id| self.installs.progress.get(id)) {
            Some(Some(p)) => tr!("Installing… {percent} %", percent = (p * 100.0).floor() as u32),
            Some(None) => tr!("Installing…").to_owned(),
            None => tr!("Not installed").to_owned(),
        }
    }
}
