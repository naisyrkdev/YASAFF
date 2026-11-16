//! Lutris games (Linux): importing the games installed in Lutris, and, with sync on, following
//! Lutris: a game added to it shows up here, one removed from it leaves the home screen (kept with
//! its images in `Library::uninstalled`, and back if it returns to Lutris).

use super::*;
use crate::lutris;
use std::time::SystemTime;

/// How often Lutris's game database is checked for changes while syncing.
const CHECK_EVERY: Duration = Duration::from_secs(5);

/// A finished listing: whether it syncs, whether its outcome is announced, and the games.
type Listing = (bool, bool, Result<Vec<lutris::Game>, String>);

#[derive(Default)]
pub(super) struct LutrisSync {
    /// A listing in progress: whether it syncs (removing games gone from Lutris), whether its
    /// outcome is announced, and the games.
    running: Option<mpsc::Receiver<Listing>>,
    /// The database's change time when it was last read.
    read_at: Option<SystemTime>,
    checked: Option<Instant>,
}

impl App {
    /// While sync is on, re-reads Lutris's games whenever its database changes.
    pub(super) fn poll_lutris(&mut self) {
        self.apply_lutris();
        if !self.settings.lutris_sync || self.lutris.running.is_some() || self.lutris.checked.is_some_and(|at| at.elapsed() < CHECK_EVERY) {
            return;
        }
        self.lutris.checked = Some(Instant::now());
        let changed = lutris::changed_at();
        if changed.is_some() && changed != self.lutris.read_at {
            self.lutris.read_at = changed;
            self.sync_lutris(false);
        }
    }

    /// Reads Lutris's games now and syncs the library to them; `announce` tells the outcome even
    /// when nothing changed.
    pub(super) fn sync_lutris(&mut self, announce: bool) {
        self.list_lutris(true, announce);
    }

    /// Adds the games installed in Lutris once, without removing any.
    pub(super) fn import_lutris(&mut self) {
        self.list_lutris(false, true);
    }

    fn list_lutris(&mut self, sync: bool, announce: bool) {
        if self.lutris.running.is_some() {
            return;
        }
        let Some(command) = crate::stores::lutris_command() else {
            if announce {
                self.notify(tr!("Lutris is not installed"));
            }
            return;
        };
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let games = lutris::installed_games(&command).map_err(|e| format!("{e:#}"));
            let _ = tx.send((sync, announce, games));
            ctx.request_repaint();
        });
        self.lutris.running = Some(rx);
    }

    fn apply_lutris(&mut self) {
        let Some(rx) = &self.lutris.running else { return };
        let (sync, announce, games) = match rx.try_recv() {
            Ok(listed) => listed,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => (false, false, Err("Lutris listing stopped".into())),
        };
        self.lutris.running = None;
        let games = match games {
            Ok(games) => games,
            Err(e) => {
                log::warn!("reading Lutris games: {e}");
                if announce {
                    self.notify(tr!("Could not read Lutris games: {error}", error = e));
                }
                return;
            }
        };
        let Some(command) = crate::stores::lutris_command() else { return };
        let selected = self.selected_game().map(|g| g.id.clone());
        let id_of = |g: &Game| lutris::id_in(&g.args);
        let mut added = Vec::new();
        for game in &games {
            // Also skips a game added from Lutris's own shortcut, which names it by slug.
            let by_slug = format!("lutris:rungame/{}", game.slug);
            if self.library.games.iter().any(|g| id_of(g) == Some(game.id) || g.args.iter().chain([&g.target]).any(|a| a.contains(&by_slug))) {
                continue;
            }
            if let Some(back) = self.library.uninstalled.iter().find(|g| id_of(g) == Some(game.id)).map(|g| g.id.clone()) {
                self.library.move_to_installed(&back);
                continue;
            }
            let mut entry = scan::game_entry(game.name.clone(), command[0].clone(), game.cover());
            entry.args = command[1..].iter().cloned().chain([game.uri()]).collect();
            let i = self.add_entry(&entry);
            added.push(self.library.games[i].id.clone());
        }
        let mut removed = 0;
        if sync {
            let listed: Vec<u64> = games.iter().map(|g| g.id).collect();
            let running = self.game.as_ref().map(|g| g.game_id.clone());
            while let Some(i) = self
                .library
                .games
                .iter()
                .position(|g| id_of(g).is_some_and(|id| !listed.contains(&id)) && Some(&g.id) != running.as_ref())
            {
                self.library.move_to_uninstalled(i);
                removed += 1;
            }
        }
        self.reselect(selected);
        if !added.is_empty() || removed > 0 {
            self.save();
        }
        let n = added.len();
        if n > 0 {
            self.notify(tr_n!(n, "Added {n} game from Lutris", "Added {n} games from Lutris"));
        } else if announce {
            self.notify(tr!("No new games in Lutris"));
        }
    }
}
