//! Steam games owned but not installed: starting their install through Steam and following it.
//! A background thread reads each Steam game's `appmanifest_<appid>.acf`, so installs and
//! uninstalls done in Steam itself (or while YASAFF was closed) show up too: a game Steam finished
//! installing joins the home screen, and one uninstalled in Steam moves to the Library tile.

use super::*;
use crate::steam::{self, Install, Progress};
use std::collections::HashMap;

/// How often games that aren't installed are checked; installed ones are checked
/// `INSTALLED_EVERY` times less often, since they rarely change.
const TICK: Duration = Duration::from_secs(3);
const INSTALLED_EVERY: u32 = 10;
/// How long an installing game waits for Steam to start on it (the user confirming Steam's install
/// dialog) before it goes back to the Library.
const START_GRACE: Duration = Duration::from_secs(10 * 60);
/// How often the list of Steam games in the library is handed to the watcher again.
const RESYNC: Duration = Duration::from_secs(2);

/// A game in a store YASAFF can install from: its Steam appid, or Heroic's runner and app name.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) enum StoreGame {
    Steam(String),
    Heroic { runner: String, app: String },
}

impl StoreGame {
    /// The store game a library game launches, `None` for anything else.
    pub(super) fn of(game: &Game) -> Option<StoreGame> {
        if game.app_id.is_some() {
            return None;
        }
        let ids = crate::physical::ids::from_command(&game.target, &game.args);
        ids.into_iter().find_map(|id| {
            let runner = match id.store.as_str() {
                crate::physical::ids::STEAM => return Some(StoreGame::Steam(id.id)),
                crate::physical::ids::EPIC => "legendary",
                crate::physical::ids::GOG => "gog",
                crate::physical::ids::AMAZON => "nile",
                _ => return None,
            };
            Some(StoreGame::Heroic { runner: runner.to_owned(), app: id.id })
        })
    }

    /// What starts the game.
    pub(super) fn target(&self) -> String {
        match self {
            StoreGame::Steam(appid) => steam::run_uri(appid),
            StoreGame::Heroic { runner, app } => scan::heroic_uri(runner, app),
        }
    }

    pub(super) fn store_name(&self) -> &'static str {
        match self {
            StoreGame::Steam(_) => "Steam",
            StoreGame::Heroic { runner, .. } if runner == "legendary" => "Epic",
            StoreGame::Heroic { runner, .. } if runner == "gog" => "GOG",
            StoreGame::Heroic { .. } => "Amazon",
        }
    }

    fn is_heroic(&self) -> bool {
        matches!(self, StoreGame::Heroic { .. })
    }
}

/// One library game the watcher follows: its id, its store game and whether the library thinks
/// it is installed.
#[derive(Clone, PartialEq)]
struct Watched {
    game_id: String,
    key: StoreGame,
    installed: bool,
}

pub(super) struct Installs {
    /// The account last logged into Steam here; read once, and again when Settings asks.
    pub(super) steam_id: Option<String>,
    /// The account signed in with the QR code, by name, for Settings.
    pub(super) signed_in: Option<String>,
    /// Games Steam is downloading now, by game id.
    pub(super) progress: HashMap<String, Progress>,
    /// When each installing game was handed to Steam, for games Steam hasn't started on yet.
    waiting_since: HashMap<String, Instant>,
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
        Installs { steam_id: steam::detect_steam_id(), signed_in, progress: HashMap::new(), waiting_since: HashMap::new(), watch, updates, sent: Vec::new(), synced: None }
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
        // Without readable Steam libraries or Heroic data (not installed, drive not mounted),
        // nothing is said about that store's games.
        let wants = |heroic: bool| games.iter().any(|g| g.key.is_heroic() == heroic);
        let libraries = if wants(false) { steam::libraries() } else { None };
        let heroic = if wants(true) { scan::heroic_installed() } else { None };
        let mut changed = false;
        for game in &games {
            if game.installed && tick % INSTALLED_EVERY != 1 && known.contains_key(&game.game_id) {
                continue;
            }
            let status = match &game.key {
                StoreGame::Steam(appid) => match &libraries {
                    Some(libraries) => steam::install_status(libraries, appid),
                    None => continue,
                },
                StoreGame::Heroic { runner, app } => match &heroic {
                    Some(installed) if installed.contains(&(runner.clone(), app.clone())) => Install::Installed,
                    Some(_) => Install::NotInstalled,
                    None => continue,
                },
            };
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
    /// Hands the watcher the library's Steam games when they changed, and applies what it found:
    /// a game Steam starts installing moves from the Library to the home screen, one it finished
    /// becomes playable, and one that is gone (uninstalled, or its install canceled) moves to the
    /// Library.
    pub(super) fn poll_installs(&mut self) {
        if self.installs.synced.is_none_or(|at| at.elapsed() >= RESYNC) {
            self.installs.synced = Some(Instant::now());
            let watched = |games: &[Game], home: bool| -> Vec<Watched> {
                games
                    .iter()
                    .filter_map(|g| Some(Watched { game_id: g.id.clone(), key: StoreGame::of(g)?, installed: home && !g.installing }))
                    .collect()
            };
            let mut list = watched(&self.library.games, true);
            list.extend(watched(&self.library.uninstalled, false));
            if list != self.installs.sent {
                let _ = self.installs.watch.send(list.clone());
                self.installs.sent = list;
            }
            self.give_up_waiting();
        }
        let mut save = false;
        while let Ok((game_id, status)) = self.installs.updates.try_recv() {
            save |= self.apply_install(&game_id, status);
        }
        if save {
            self.installs.sent.clear();
            self.save();
        }
    }

    /// Applies one status from the watcher; true when the library changed.
    fn apply_install(&mut self, game_id: &str, status: Install) -> bool {
        let home = self.library.position(game_id);
        let installing = home.is_some_and(|i| self.library.games[i].installing);
        match status {
            Install::Installed => {
                self.installs.progress.remove(game_id);
                self.installs.waiting_since.remove(game_id);
                let i = match home {
                    Some(i) if installing => i,
                    Some(_) => return false,
                    None => match self.move_home(game_id) {
                        Some(i) => i,
                        None => return false,
                    },
                };
                let game = &mut self.library.games[i];
                game.installing = false;
                let (name, has_cover) = (game.name.clone(), game.has_cover());
                self.notify(tr!("{name} is installed and ready to play", name = name));
                if !has_cover {
                    self.fetch_artwork(&[game_id.to_owned()]);
                }
                true
            }
            Install::Downloading(progress) => {
                // An installed game Steam is updating also lacks the "fully installed" flag for a
                // while; it stays playable.
                if home.is_some() && !installing {
                    return false;
                }
                self.installs.progress.insert(game_id.to_owned(), progress);
                self.installs.waiting_since.remove(game_id);
                match home {
                    Some(_) => false,
                    None => {
                        let Some(i) = self.move_home(game_id) else { return false };
                        self.library.games[i].installing = true;
                        true
                    }
                }
            }
            Install::NotInstalled => {
                let had_progress = self.installs.progress.remove(game_id).is_some();
                let Some(i) = home else { return false };
                let running = self.game.as_ref().is_some_and(|g| g.game_id == game_id);
                // Heroic leaves nothing on disk until an install is done, so its installing games
                // wait; so does a game handed to Steam that Steam hasn't started on yet.
                let heroic = StoreGame::of(&self.library.games[i]).is_some_and(|k| k.is_heroic());
                if running || (installing && (heroic || (!had_progress && self.installs.waiting_since.contains_key(game_id)))) {
                    return false;
                }
                if installing {
                    let name = self.library.games[i].name.clone();
                    self.notify(tr!("Installing {name} was canceled", name = name));
                }
                self.move_to_library(i);
                true
            }
        }
    }

    /// Installing games Steam never started on go back to the Library after a while.
    fn give_up_waiting(&mut self) {
        let now = Instant::now();
        let stale: Vec<usize> = (0..self.library.games.len())
            .filter(|&i| {
                let game = &self.library.games[i];
                game.installing && !self.installs.progress.contains_key(&game.id) && StoreGame::of(game).is_some_and(|k| !k.is_heroic())
            })
            .filter(|&i| {
                let since = *self.installs.waiting_since.entry(self.library.games[i].id.clone()).or_insert(now);
                since.elapsed() >= START_GRACE
            })
            .collect();
        for &i in stale.iter().rev() {
            self.installs.waiting_since.remove(&self.library.games[i].id);
            self.move_to_library(i);
        }
        if !stale.is_empty() {
            self.save();
        }
    }

    /// Moves an uninstalled game to the home screen, keeping the selection on the same game.
    fn move_home(&mut self, game_id: &str) -> Option<usize> {
        let selected = self.selected_game().map(|g| g.id.clone());
        self.library.move_to_installed(game_id)?;
        self.reselect(selected);
        self.library.position(game_id)
    }

    /// Moves the home screen game at `i` to the Library, keeping the selection on the same game.
    fn move_to_library(&mut self, i: usize) {
        let selected = self.selected_game().map(|g| g.id.clone());
        self.library.move_to_uninstalled(i);
        self.reselect(selected);
    }

    /// Selects the game with `id` again after games moved, or keeps the selection in range.
    pub(super) fn reselect(&mut self, id: Option<String>) {
        if let Some(i) = id.and_then(|id| self.library.position(&id)) {
            self.selected = i;
        }
        self.clamp_selection();
    }

    /// Asks the store to install a game: Steam shows its install dialog, Heroic offers to install a
    /// game it is asked to start that isn't installed. The game goes to the home screen right away,
    /// marked as installing, and is playable once the store is done.
    pub(super) fn install_store_game(&mut self, key: &StoreGame, name: &str) {
        let id = match self.library.uninstalled.iter().find(|g| StoreGame::of(g).as_ref() == Some(key)) {
            Some(game) => game.id.clone(),
            None => self.library.add_uninstalled(&scan::game_entry(name.to_owned(), key.target(), None)),
        };
        let (store, uri) = match key {
            StoreGame::Steam(appid) => (Store::Steam, steam::install_uri(appid)),
            StoreGame::Heroic { .. } => (Store::Heroic, key.target()),
        };
        let result = match self.settings.store_command(store) {
            Some(command) => store.launch_uri(&command, &uri),
            None => open::that_detached(&uri).map_err(anyhow::Error::from),
        };
        if let Err(e) = result {
            self.save();
            return self.notify(tr!("Could not ask {store} to install {name}: {error}", store = store.short_name(), name = name, error = format!("{e:#}")));
        }
        if let Some(i) = self.move_home(&id) {
            let game = &mut self.library.games[i];
            game.installing = true;
            if !game.has_cover() {
                self.fetch_artwork(std::slice::from_ref(&id));
            }
        }
        self.installs.waiting_since.insert(id, Instant::now());
        self.installs.sent.clear();
        self.save();
        self.notify(tr!("{store} is installing {name}. Confirm in its window", store = store.short_name(), name = name));
    }

    /// Stops following an install and puts the selected game back in the Library.
    pub(super) fn stop_install_selected(&mut self) {
        let Some(game) = self.selected_game().filter(|g| g.installing) else { return };
        let (id, name) = (game.id.clone(), game.name.clone());
        self.installs.progress.remove(&id);
        self.installs.waiting_since.remove(&id);
        self.move_to_library(self.selected);
        self.installs.sent.clear();
        self.save();
        self.notify(tr!("{name} is back in the Library", name = name));
    }

    /// How far Steam is with an installing game.
    pub(super) fn install_label(&self, game_id: &str) -> String {
        match self.installs.progress.get(game_id) {
            Some(Progress { paused: true, fraction: Some(p) }) => tr!("Paused at {percent} %", percent = (p * 100.0).floor() as u32),
            Some(Progress { paused: true, fraction: None }) => tr!("Paused").to_owned(),
            Some(Progress { fraction: Some(p), .. }) => tr!("Installing… {percent} %", percent = (p * 100.0).floor() as u32),
            Some(Progress { fraction: None, .. }) => tr!("Installing…").to_owned(),
            None if self.library.position(game_id).and_then(|i| StoreGame::of(&self.library.games[i])).is_some_and(|k| k.is_heroic()) => {
                tr!("Installing in Heroic…").to_owned()
            }
            None => tr!("Waiting for Steam…").to_owned(),
        }
    }
}
