//! The Library tile: games the user owns in Steam, Epic or GOG that aren't installed, to install
//! from here. The lists come straight from Steam (by the method picked in Settings) and from
//! Heroic's library files; a game being installed moves to the home screen.

use super::*;
use crate::steam;
use installs::StoreGame;
use std::collections::{HashMap, HashSet};

/// Rows Left/Right jump by.
const PAGE: usize = 8;

/// One owned game a store listed.
#[derive(Clone)]
pub(super) struct Listed {
    key: StoreGame,
    name: String,
    /// The store's cover address, when it gives one.
    art: Option<String>,
}

#[derive(Default)]
pub(super) struct LibraryView {
    owned: Vec<Listed>,
    fetching: Option<mpsc::Receiver<(Vec<Listed>, Vec<String>)>>,
    warnings: Vec<String>,
    cursor: usize,
    scroll: usize,
    /// Cover image address by game: a cached file when there is one, else the store's web address.
    covers: HashMap<StoreGame, Option<String>>,
    /// The game the install dialog is about.
    pub(super) pending: Option<Listed>,
}

impl App {
    /// Whether any store's games that aren't installed are listed: the Library tile shows then.
    pub(super) fn library_enabled(&self) -> bool {
        self.settings.steam_show_owned || self.settings.heroic_show_owned
    }

    /// Owned games not on the home screen, by name: the fetched lists plus games kept in
    /// `Library::uninstalled` (uninstalled in their store, or whose install was canceled).
    pub(super) fn library_rows(&self) -> Vec<Listed> {
        let home: HashSet<StoreGame> = self.library.games.iter().filter_map(StoreGame::of).collect();
        let mut rows: Vec<Listed> = self.library_view.owned.iter().filter(|g| !home.contains(&g.key)).cloned().collect();
        let listed: HashSet<StoreGame> = rows.iter().map(|r| r.key.clone()).collect();
        let shown = |key: &StoreGame| match key {
            StoreGame::Steam(_) => self.settings.steam_show_owned,
            StoreGame::Heroic { .. } => self.settings.heroic_show_owned,
        };
        for game in &self.library.uninstalled {
            if let Some(key) = StoreGame::of(game).filter(|k| shown(k) && !listed.contains(k) && !home.contains(k)) {
                rows.push(Listed { key, name: game.name.clone(), art: None });
            }
        }
        rows.sort_by_key(|r| r.name.to_lowercase());
        rows
    }

    pub(super) fn open_library(&mut self) {
        self.mode = Mode::Library;
        if self.library_view.fetching.is_none() {
            self.refresh_library();
        }
    }

    fn refresh_library(&mut self) {
        self.installs.refresh_steam_id();
        let steam_account = self.settings.steam_show_owned.then(|| steam::Account {
            source: self.settings.steam_source,
            api_key: self.settings.steam_api_key.clone(),
            detected_id: self.installs.steam_id.clone(),
        });
        let heroic = self.settings.heroic_show_owned;
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let mut games = Vec::new();
            let mut warnings = Vec::new();
            if let Some(account) = steam_account {
                let owned = steam::owned_games(&account);
                warnings.extend(owned.warning);
                games.extend(owned.games.into_iter().map(|g| Listed { key: StoreGame::Steam(g.appid), name: g.name, art: None }));
            }
            if heroic {
                let installed = scan::heroic_installed();
                let owned = scan::heroic_owned();
                if installed.is_none() || owned.is_empty() {
                    warnings.push(tr!("No Epic or GOG games found. Open Heroic and log into Epic and GOG").to_owned());
                }
                let installed = installed.unwrap_or_default();
                games.extend(
                    owned
                        .into_iter()
                        .filter(|g| !installed.contains(&(g.runner.to_owned(), g.app.clone())))
                        .map(|g| Listed { key: StoreGame::Heroic { runner: g.runner.to_owned(), app: g.app }, name: g.title, art: g.art }),
                );
            }
            let _ = tx.send((games, warnings));
            ctx.request_repaint();
        });
        self.library_view.fetching = Some(rx);
    }

    pub(super) fn poll_library(&mut self) {
        let Some(rx) = &self.library_view.fetching else { return };
        let (games, warnings) = match rx.try_recv() {
            Ok(listed) => listed,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => (Vec::new(), Vec::new()),
        };
        let view = &mut self.library_view;
        view.fetching = None;
        // A failed refresh keeps the list already shown.
        if !games.is_empty() || warnings.is_empty() {
            view.owned = games;
        }
        view.warnings = warnings;
    }

    pub(super) fn handle_library(&mut self, action: Action) {
        let rows = self.library_rows();
        let view = &mut self.library_view;
        let last = rows.len().saturating_sub(1);
        match action {
            Action::Up => view.cursor = view.cursor.saturating_sub(1),
            Action::Down => view.cursor = (view.cursor + 1).min(last),
            Action::Left => view.cursor = view.cursor.saturating_sub(PAGE),
            Action::Right => view.cursor = (view.cursor + PAGE).min(last),
            Action::Options => self.refresh_library(),
            Action::Back => self.mode = Mode::Home,
            Action::Confirm => {
                let Some(row) = rows.get(view.cursor) else { return };
                view.pending = Some(row.clone());
                self.mode = Mode::ConfirmRemove { what: Removal::InstallGame, yes: true };
            }
            _ => {}
        }
    }

    /// Installs the game the install dialog was about.
    pub(super) fn install_pending(&mut self) {
        if let Some(game) = self.library_view.pending.take() {
            self.install_store_game(&game.key, &game.name);
        }
    }

    /// The name of the game the install dialog is about.
    pub(super) fn pending_name(&self) -> String {
        self.library_view.pending.as_ref().map(|g| g.name.clone()).unwrap_or_default()
    }

    /// Where a game's cover comes from: the store's files on this computer, or its website.
    fn library_cover(&mut self, game: &Listed) -> Option<String> {
        if let Some(uri) = self.library_view.covers.get(&game.key) {
            return uri.clone();
        }
        let local = match &game.key {
            StoreGame::Steam(appid) => scan::steam_roots().into_iter().find_map(|r| crate::metadata::steam_cover_in(&r.join("appcache/librarycache").join(appid))),
            key => crate::metadata::local(&crate::physical::ids::from_command(&key.target(), &[])).and_then(|f| f.cover),
        };
        let uri = match (local, &game.key) {
            (Some(path), _) => Some(file_uri(&path)),
            (None, StoreGame::Steam(appid)) => Some(steam::cdn_art(appid)[0].1.clone()),
            (None, _) => game.art.clone(),
        };
        self.library_view.covers.insert(game.key.clone(), uri.clone());
        uri
    }

    pub(super) fn draw_library(&mut self, ui: &Ui, screen: Rect, s: f32) {
        if !matches!(self.mode, Mode::Library) {
            return;
        }
        let rows = self.library_rows();
        let painter = ui.painter().clone();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(170));
        let panel = Rect::from_center_size(screen.center(), vec2((1100.0 * s).min(screen.width() - 40.0), screen.height() - 150.0 * s));
        painter.rect_filled(panel, (8.0 * s) as u8, Color32::from_rgb(14, 20, 36));
        painter.text(panel.left_top() + vec2(40.0 * s, 36.0 * s), Align2::LEFT_TOP, tr!("Library"), FontId::proportional(30.0 * s), Color32::WHITE);

        let font = FontId::proportional(20.0 * s);
        let gray = Color32::from_gray(170);
        let status_pos = pos2(panel.left() + 40.0 * s, panel.top() + 104.0 * s);
        let fetching = self.library_view.fetching.is_some();
        let mut status_x = status_pos.x;
        if fetching {
            let spinner = Rect::from_center_size(status_pos + vec2(12.0 * s, 0.0), vec2(24.0 * s, 24.0 * s));
            egui::Spinner::new().size(24.0 * s).color(Color32::WHITE).paint_at(ui, spinner);
            status_x += 40.0 * s;
        }
        let status = if fetching && rows.is_empty() {
            tr!("Getting your games…").to_owned()
        } else {
            let n = rows.len();
            tr_n!(n, "{n} game you own isn't installed", "{n} games you own aren't installed")
        };
        painter.text(pos2(status_x, status_pos.y), Align2::LEFT_CENTER, status, font.clone(), gray);
        let mut list_top = status_pos.y + 36.0 * s;
        for warning in &self.library_view.warnings {
            let clip = painter.with_clip_rect(panel.shrink(20.0 * s));
            clip.text(pos2(status_pos.x, list_top - 4.0 * s), Align2::LEFT_CENTER, warning, FontId::proportional(16.0 * s), PAD_Y);
            list_top += 28.0 * s;
        }

        let row_h = 92.0 * s;
        let visible = (((panel.bottom() - 24.0 * s - list_top) / row_h).floor() as usize).max(1);
        let wheel = wheel_lines(ui);
        let view = &mut self.library_view;
        if wheel.abs() >= 0.5 {
            view.cursor = view.cursor.saturating_add_signed(-(wheel.round().clamp(-5.0, 5.0) as isize));
        }
        view.cursor = view.cursor.min(rows.len().saturating_sub(1));
        if view.cursor < view.scroll {
            view.scroll = view.cursor;
        } else if view.cursor >= view.scroll + visible {
            view.scroll = view.cursor + 1 - visible;
        }
        view.scroll = view.scroll.min(rows.len().saturating_sub(visible));
        let (cursor, scroll) = (view.cursor, view.scroll);

        if !fetching && rows.is_empty() {
            let text = tr!("Every game you own is installed. Press Menu / Tab to check again.");
            painter.text(pos2(panel.center().x, list_top + 60.0 * s), Align2::CENTER_CENTER, text, font, Color32::from_gray(150));
        }
        let mut hovered = None;
        let mut clicked = None;
        for (i, row) in rows.iter().enumerate().skip(scroll).take(visible) {
            let r = Rect::from_min_size(pos2(panel.left() + 20.0 * s, list_top + (i - scroll) as f32 * row_h), vec2(panel.width() - 40.0 * s, row_h - 6.0 * s));
            if i == cursor {
                painter.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(22));
                painter.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
            }
            let cover_h = r.height() - 12.0 * s;
            let icon = Rect::from_min_size(pos2(r.left() + 24.0 * s, r.top() + 6.0 * s), vec2(cover_h * 2.0 / 3.0, cover_h));
            painter.rect_filled(icon, (4.0 * s) as u8, placeholder_color(&row.name));
            painter.text(icon.center(), Align2::CENTER_CENTER, initials(&row.name), FontId::proportional(icon.height() * 0.3), Color32::WHITE);
            if let Some(uri) = self.library_cover(row) {
                paint_placed(ui, egui::Image::new(uri), icon, 1.0, (4.0 * s) as u8, CoverPlacement::default());
            }

            let color = Color32::from_gray(200);
            let tag = painter.layout_no_wrap(tr!("Not installed").to_owned(), FontId::proportional(15.0 * s), color);
            let t = Rect::from_min_size(pos2(r.right() - 40.0 * s - tag.size().x, r.center().y - 13.0 * s), tag.size() + vec2(20.0 * s, 10.0 * s));
            painter.rect_filled(t, (t.height() / 2.0) as u8, Color32::from_white_alpha(30));
            painter.galley(t.center() - tag.size() / 2.0, tag, color);

            let text_x = icon.right() + 18.0 * s;
            let clip = painter.with_clip_rect(Rect::from_min_max(r.min, pos2(t.left() - 12.0 * s, r.bottom())));
            clip.text(pos2(text_x, r.top() + 14.0 * s), Align2::LEFT_TOP, &row.name, FontId::proportional(22.0 * s), Color32::WHITE);
            clip.text(pos2(text_x, r.top() + 44.0 * s), Align2::LEFT_TOP, row.key.store_name(), FontId::proportional(15.0 * s), Color32::from_gray(140));

            let resp = ui.interact(r, Id::new(("library", i)), Sense::click());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                hovered = Some(i);
            }
            if resp.clicked() {
                clicked = Some(i);
            }
        }
        if let Some(i) = hovered {
            self.library_view.cursor = i;
        }
        if let Some(i) = clicked {
            self.library_view.cursor = i;
            self.handle(ui.ctx(), Action::Confirm);
        }
    }
}
