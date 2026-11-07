//! The Library tile: Steam games the account owns that aren't installed, to install from here.
//! The list comes straight from Steam (by the method picked in Settings); a game Steam finishes
//! installing joins the home screen.

use super::*;
use crate::steam::{self, OwnedGame, Owned};
use std::collections::{HashMap, HashSet};

/// Rows Left/Right jump by.
const PAGE: usize = 8;

#[derive(Default)]
pub(super) struct LibraryView {
    owned: Vec<OwnedGame>,
    fetching: Option<mpsc::Receiver<Owned>>,
    warning: Option<String>,
    cursor: usize,
    scroll: usize,
    /// Cover image address by appid: Steam's cached file when there is one, else its CDN.
    covers: HashMap<String, String>,
}

/// One game in the Library list.
pub(super) struct LibraryRow {
    appid: String,
    name: String,
    /// Set once an install was started from here: the game waiting in `Library::uninstalled`.
    game_id: Option<String>,
}

impl App {
    /// Steam games owned but not on the home screen, by name: the fetched list plus games
    /// waiting in `Library::uninstalled` (being installed, or uninstalled in Steam).
    pub(super) fn library_rows(&self) -> Vec<LibraryRow> {
        let home: HashSet<String> = self.library.games.iter().filter_map(Self::steam_appid).collect();
        let waiting: HashMap<String, &Game> = self.library.uninstalled.iter().filter_map(|g| Some((Self::steam_appid(g)?, g))).collect();
        let mut rows: Vec<LibraryRow> = self
            .library_view
            .owned
            .iter()
            .filter(|g| !home.contains(&g.appid))
            .map(|g| LibraryRow { appid: g.appid.clone(), name: g.name.clone(), game_id: waiting.get(&g.appid).map(|w| w.id.clone()) })
            .collect();
        let listed: HashSet<String> = rows.iter().map(|r| r.appid.clone()).collect();
        for (appid, game) in waiting.into_iter().filter(|(appid, _)| !listed.contains(appid) && !home.contains(appid)) {
            rows.push(LibraryRow { appid, name: game.name.clone(), game_id: Some(game.id.clone()) });
        }
        rows.sort_by_key(|r| r.name.to_lowercase());
        rows
    }

    /// The name of an owned game, for the install dialog.
    pub(super) fn library_game_name(&self, appid: u32) -> String {
        let appid = appid.to_string();
        let owned = self.library_view.owned.iter().find(|g| g.appid == appid).map(|g| g.name.clone());
        owned
            .or_else(|| self.library.uninstalled.iter().find(|g| Self::steam_appid(g).as_deref() == Some(appid.as_str())).map(|g| g.name.clone()))
            .unwrap_or(appid)
    }

    pub(super) fn open_library(&mut self) {
        self.mode = Mode::Library;
        if self.library_view.fetching.is_none() {
            self.refresh_library();
        }
    }

    fn refresh_library(&mut self) {
        self.installs.refresh_steam_id();
        let account = steam::Account {
            source: self.settings.steam_source,
            api_key: self.settings.steam_api_key.clone(),
            detected_id: self.installs.steam_id.clone(),
        };
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(steam::owned_games(&account));
            ctx.request_repaint();
        });
        self.library_view.fetching = Some(rx);
    }

    pub(super) fn poll_library(&mut self) {
        let Some(rx) = &self.library_view.fetching else { return };
        let owned = match rx.try_recv() {
            Ok(owned) => owned,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Owned { games: Vec::new(), warning: None },
        };
        let view = &mut self.library_view;
        view.fetching = None;
        // A failed refresh keeps the list already shown.
        if !owned.games.is_empty() || owned.warning.is_none() {
            view.owned = owned.games;
        }
        view.warning = owned.warning;
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
                if row.game_id.as_ref().is_some_and(|id| self.installs.progress.contains_key(id)) {
                    let status = self.install_label(row.game_id.as_deref());
                    return self.notify(tr!("{name}: {status}", name = row.name, status = status));
                }
                let Ok(appid) = row.appid.parse() else { return };
                self.mode = Mode::ConfirmRemove { what: Removal::InstallGame(appid), yes: true };
            }
            _ => {}
        }
    }

    /// Where a game's cover comes from: Steam's cache on this computer, or Steam's CDN.
    fn library_cover(&mut self, appid: &str) -> String {
        if let Some(uri) = self.library_view.covers.get(appid) {
            return uri.clone();
        }
        let cached = scan::steam_roots().into_iter().find_map(|r| crate::metadata::steam_cover_in(&r.join("appcache/librarycache").join(appid)));
        let uri = match cached {
            Some(path) => file_uri(&path),
            None => steam::cdn_art(appid)[0].1.clone(),
        };
        self.library_view.covers.insert(appid.to_owned(), uri.clone());
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
            tr!("Getting your Steam games…").to_owned()
        } else {
            let n = rows.len();
            tr_n!(n, "{n} game you own isn't installed", "{n} games you own aren't installed")
        };
        painter.text(pos2(status_x, status_pos.y), Align2::LEFT_CENTER, status, font.clone(), gray);
        let mut list_top = status_pos.y + 36.0 * s;
        if let Some(warning) = &self.library_view.warning {
            let clip = painter.with_clip_rect(panel.shrink(20.0 * s));
            clip.text(pos2(status_pos.x, status_pos.y + 32.0 * s), Align2::LEFT_CENTER, warning, FontId::proportional(16.0 * s), PAD_Y);
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
            let text = tr!("Every Steam game you own is installed. Press Menu / Tab to check again.");
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
            let uri = self.library_cover(&row.appid);
            paint_placed(ui, egui::Image::new(uri), icon, 1.0, (4.0 * s) as u8, CoverPlacement::default());

            let label = self.install_label(row.game_id.as_deref());
            let installing = row.game_id.as_ref().is_some_and(|id| self.installs.progress.contains_key(id));
            let color = if installing { PAD_A } else { Color32::from_gray(200) };
            let tag = painter.layout_no_wrap(label, FontId::proportional(15.0 * s), color);
            let t = Rect::from_min_size(pos2(r.right() - 40.0 * s - tag.size().x, r.center().y - 13.0 * s), tag.size() + vec2(20.0 * s, 10.0 * s));
            painter.rect_filled(t, (t.height() / 2.0) as u8, Color32::from_white_alpha(30));
            painter.galley(t.center() - tag.size() / 2.0, tag, color);
            if let Some(Some(p)) = row.game_id.as_ref().and_then(|id| self.installs.progress.get(id)) {
                let bar = Rect::from_min_size(pos2(icon.right() + 18.0 * s, r.bottom() - 18.0 * s), vec2((t.left() - icon.right() - 40.0 * s).max(0.0), 6.0 * s));
                painter.rect_filled(bar, (3.0 * s) as u8, Color32::from_white_alpha(30));
                painter.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * p.clamp(0.0, 1.0), bar.height())), (3.0 * s) as u8, PAD_A);
            }

            let text_x = icon.right() + 18.0 * s;
            let clip = painter.with_clip_rect(Rect::from_min_max(r.min, pos2(t.left() - 12.0 * s, r.bottom())));
            clip.text(pos2(text_x, r.top() + 14.0 * s), Align2::LEFT_TOP, &row.name, FontId::proportional(22.0 * s), Color32::WHITE);
            clip.text(pos2(text_x, r.top() + 44.0 * s), Align2::LEFT_TOP, "Steam", FontId::proportional(15.0 * s), Color32::from_gray(140));

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
