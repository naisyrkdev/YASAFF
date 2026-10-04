//! The Addons Manager: a window opened from Settings → Addons. Its first page offers the
//! installed addons and importing a new one; the installed list leads to one addon's page
//! with its options (bar placement, replaceable images, uninstall).

use super::*;
use crate::appstore::{self, Manifest};
use crate::settings::AddonSettings;

#[derive(Clone, PartialEq)]
pub(super) enum AddonView {
    Menu,
    Installed,
    /// By id, so the page survives the list being re-sorted by an install.
    Addon(String),
    /// Moving a widget placed anywhere on screen; `from` is its position and size before, for Back.
    Move { id: String, from: (f32, f32, u32) },
}

/// How far one press of a direction moves a widget, as a fraction of the screen.
const MOVE_STEP: f32 = 0.01;
const SIZE_STEP: i32 = 10;

pub(super) struct AddonManager {
    pub(super) view: AddonView,
    pub(super) cursor: usize,
}

impl AddonManager {
    pub(super) fn open(view: AddonView, cursor: usize) -> Mode {
        Mode::AddonManager(AddonManager { view, cursor })
    }
}

#[derive(Clone, Copy, PartialEq)]
enum AddonRow {
    Installed,
    Import,
    Addon(usize),
    NoAddons,
    Place,
    Size,
    Position,
    /// Index into the addon's `widget.images`.
    Image(usize),
    ResetImages,
    Uninstall,
}

impl App {
    /// The addon a page is about, with its index in `self.addons.installed`.
    fn managed_addon(&self, view: &AddonView) -> Option<(usize, &Manifest)> {
        let (AddonView::Addon(id) | AddonView::Move { id, .. }) = view else { return None };
        self.addons.installed.iter().enumerate().find(|(_, m)| &m.id == id)
    }

    fn addon_rows(&self, view: &AddonView) -> Vec<AddonRow> {
        match view {
            AddonView::Menu => vec![AddonRow::Installed, AddonRow::Import],
            AddonView::Installed if self.addons.installed.is_empty() => vec![AddonRow::NoAddons],
            AddonView::Installed => (0..self.addons.installed.len()).map(AddonRow::Addon).collect(),
            AddonView::Move { .. } => vec![],
            AddonView::Addon(id) => {
                let images = self.managed_addon(view).map_or(0, |(_, m)| m.widget_images().len());
                let mut rows = vec![AddonRow::Place, AddonRow::Size];
                if AddonHost::placement(&self.settings, id) == Some(Placement::Screen) {
                    rows.push(AddonRow::Position);
                }
                rows.extend((0..images).map(AddonRow::Image));
                if images > 0 {
                    rows.push(AddonRow::ResetImages);
                }
                rows.push(AddonRow::Uninstall);
                rows
            }
        }
    }

    fn placement_label(&self, id: &str) -> &'static str {
        AddonHost::placement(&self.settings, id).map_or(tr!("Off"), Placement::label)
    }

    fn addon_row_content(&self, row: AddonRow, addon: Option<&Manifest>) -> (String, String, RowValue) {
        match row {
            AddonRow::Installed => (
                tr!("Installed addons").into(),
                tr!("Place them in a bar, change their images or uninstall them").into(),
                RowValue::Summary(tr!("{count} installed", count = self.addons.installed.len())),
            ),
            AddonRow::Import => (
                tr!("Import addon from file…").into(),
                tr!("A .zip with yasaffConfig.json; the Store's Addons tab has more").into(),
                RowValue::Action(tr!("Choose…")),
            ),
            AddonRow::NoAddons => (
                tr!("No addons installed").into(),
                tr!("Import one from a file or get one from the Store's Addons tab").into(),
                RowValue::Action(tr!("Import…")),
            ),
            AddonRow::Addon(i) => {
                let Some(m) = self.addons.installed.get(i) else { return (String::new(), String::new(), RowValue::Summary(String::new())) };
                let sub = match &m.summary {
                    Some(summary) => format!("{} · {summary}", m.version),
                    None => m.version.clone(),
                };
                (m.name.clone(), sub, RowValue::Summary(self.placement_label(&m.id).into()))
            }
            AddonRow::Place => {
                let Some(m) = addon else { return (String::new(), String::new(), RowValue::Summary(String::new())) };
                let sub = match m.min_height() {
                    Some(min) => tr!("Left/Right picks the bar; it needs one at least {px} px high", px = format!("{min:.0}")),
                    None => tr!("Left/Right picks the bar it shows in").into(),
                };
                (tr!("Shown in").into(), sub, RowValue::Choice(self.placement_label(&m.id).into()))
            }
            AddonRow::Size => {
                let Some(m) = addon else { return (String::new(), String::new(), RowValue::Summary(String::new())) };
                let size = self.settings.addons.get(&m.id).cloned().unwrap_or_default().size;
                let sub = match AddonHost::placement(&self.settings, &m.id) {
                    Some(Placement::Bar(_)) => tr!("Left/Right resizes it; a bigger widget makes its bar taller"),
                    _ => tr!("Left/Right resizes it"),
                };
                (tr!("Size").into(), sub.into(), RowValue::Choice(format!("{size} %")))
            }
            AddonRow::Position => {
                let Some(m) = addon else { return (String::new(), String::new(), RowValue::Summary(String::new())) };
                let entry = self.settings.addons.get(&m.id).cloned().unwrap_or_default();
                let at = tr!("{x} % across · {y} % down", x = format!("{:.0}", entry.x * 100.0), y = format!("{:.0}", entry.y * 100.0));
                (tr!("Position on screen").into(), at, RowValue::Action(tr!("Move…")))
            }
            AddonRow::Image(j) => {
                let Some((m, image)) = addon.and_then(|m| Some((m, m.widget_images().get(j)?))) else {
                    return (String::new(), String::new(), RowValue::Summary(String::new()));
                };
                let sub = match appstore::addon_image(&m.id, &image.key) {
                    Some(_) => tr!("Your image · PNG, JPEG or WebP"),
                    None => tr!("Built-in image · PNG, JPEG or WebP"),
                };
                (image.label.clone(), sub.into(), RowValue::Action(tr!("Choose…")))
            }
            AddonRow::ResetImages => (tr!("Use built-in images").into(), tr!("Forgets the images you picked above").into(), RowValue::Action(tr!("Reset"))),
            AddonRow::Uninstall => (
                tr!("Uninstall").into(),
                tr!("Its files are deleted; its saved data is kept").into(),
                RowValue::Action(tr!("Uninstall…")),
            ),
        }
    }

    fn activate_addon_row(&mut self, row: AddonRow) {
        let Mode::AddonManager(manager) = &self.mode else { return };
        let index = self.managed_addon(&manager.view).map(|(i, _)| i);
        match (row, index) {
            (AddonRow::Installed, _) => self.mode = AddonManager::open(AddonView::Installed, 0),
            (AddonRow::Import | AddonRow::NoAddons, _) => self.import_app_file(),
            (AddonRow::Addon(i), _) => {
                if let Some(m) = self.addons.installed.get(i) {
                    self.mode = AddonManager::open(AddonView::Addon(m.id.clone()), 0);
                }
            }
            (AddonRow::Place | AddonRow::Size, _) => self.adjust_addon_row(row, 1),
            (AddonRow::Position, Some(i)) => {
                let id = self.addons.installed[i].id.clone();
                let entry = self.settings.addons.get(&id).cloned().unwrap_or_default();
                self.mode = AddonManager::open(AddonView::Move { id, from: (entry.x, entry.y, entry.size) }, 0);
            }
            (AddonRow::Image(j), Some(i)) => self.choose_addon_image(i, j),
            (AddonRow::ResetImages, Some(i)) => self.reset_addon_images(i),
            (AddonRow::Uninstall, Some(i)) => self.mode = Mode::ConfirmRemove { what: Removal::Addon(i), yes: false },
            _ => {}
        }
    }

    fn adjust_addon_row(&mut self, row: AddonRow, delta: i32) {
        let Mode::AddonManager(manager) = &self.mode else { return };
        let AddonView::Addon(id) = &manager.view else { return };
        let id = id.clone();
        match row {
            AddonRow::Place => {
                let place = cycle(&Placement::CHOICES, AddonHost::placement(&self.settings, &id), delta);
                AddonHost::set_placement(&mut self.settings, &id, place);
                self.addons.sync(&self.settings);
            }
            AddonRow::Size => self.resize_addon(&id, delta * SIZE_STEP),
            _ => return,
        }
        self.save_settings();
    }

    fn resize_addon(&mut self, id: &str, by: i32) {
        let (min, max) = AddonSettings::SIZE_RANGE;
        let entry = self.settings.addons.entry(id.to_owned()).or_default();
        entry.size = (entry.size as i32 + by).clamp(min as i32, max as i32) as u32;
    }

    fn move_addon(&mut self, id: &str, by: Vec2) {
        let entry = self.settings.addons.entry(id.to_owned()).or_default();
        entry.x = (entry.x + by.x).clamp(0.0, 1.0);
        entry.y = (entry.y + by.y).clamp(0.0, 1.0);
    }

    /// Leaves the move screen, keeping the new position or going back to the one before.
    fn finish_move(&mut self, keep: bool) {
        let Mode::AddonManager(AddonManager { view: AddonView::Move { id, from: (x, y, size) }, .. }) = &self.mode else { return };
        let (id, x, y, size) = (id.clone(), *x, *y, *size);
        if keep {
            self.save_settings();
        } else {
            let entry = self.settings.addons.entry(id.clone()).or_default();
            (entry.x, entry.y, entry.size) = (x, y, size);
        }
        let view = AddonView::Addon(id);
        let cursor = self.addon_rows(&view).iter().position(|r| *r == AddonRow::Position).unwrap_or(0);
        self.mode = AddonManager::open(view, cursor);
    }

    fn handle_addon_move(&mut self, action: Action) {
        let Mode::AddonManager(AddonManager { view: AddonView::Move { id, .. }, .. }) = &self.mode else { return };
        let id = id.clone();
        match action {
            Action::Left => self.move_addon(&id, vec2(-MOVE_STEP, 0.0)),
            Action::Right => self.move_addon(&id, vec2(MOVE_STEP, 0.0)),
            Action::Up => self.move_addon(&id, vec2(0.0, -MOVE_STEP)),
            Action::Down => self.move_addon(&id, vec2(0.0, MOVE_STEP)),
            Action::Confirm => self.finish_move(true),
            Action::Back | Action::Options => self.finish_move(false),
            _ => {}
        }
    }

    /// Back one page: an addon → the installed list → the first page → Settings.
    fn addon_manager_back(&mut self) {
        let Mode::AddonManager(manager) = &self.mode else { return };
        self.mode = match &manager.view {
            AddonView::Move { .. } => return self.finish_move(false),
            AddonView::Addon(_) => {
                let at = self.managed_addon(&manager.view).map_or(0, |(i, _)| i);
                AddonManager::open(AddonView::Installed, at)
            }
            AddonView::Installed => AddonManager::open(AddonView::Menu, 0),
            AddonView::Menu => self.settings_at(SettingRow::AddonManager),
        };
    }

    /// Where the cursor goes back to after the uninstall dialog for addon `i`.
    pub(super) fn addon_manager_after_uninstall(&self, i: usize, uninstalled: bool) -> Mode {
        match self.addons.installed.get(i) {
            Some(m) if !uninstalled => {
                let view = AddonView::Addon(m.id.clone());
                let cursor = self.addon_rows(&view).iter().position(|r| *r == AddonRow::Uninstall).unwrap_or(0);
                AddonManager::open(view, cursor)
            }
            _ => AddonManager::open(AddonView::Installed, i.min(self.addons.installed.len().saturating_sub(1))),
        }
    }

    pub(super) fn handle_addon_manager(&mut self, action: Action) {
        let Mode::AddonManager(manager) = &self.mode else { return };
        if matches!(manager.view, AddonView::Addon(_) | AddonView::Move { .. }) && self.managed_addon(&manager.view).is_none() {
            self.mode = AddonManager::open(AddonView::Installed, 0);
        }
        if let Mode::AddonManager(AddonManager { view: AddonView::Move { .. }, .. }) = &self.mode {
            return self.handle_addon_move(action);
        }
        let Mode::AddonManager(manager) = &self.mode else { return };
        let rows = self.addon_rows(&manager.view);
        let cursor = manager.cursor.min(rows.len() - 1);
        match action {
            Action::Up | Action::Down => {
                let to = if action == Action::Up { cursor.saturating_sub(1) } else { (cursor + 1).min(rows.len() - 1) };
                if let Mode::AddonManager(manager) = &mut self.mode {
                    manager.cursor = to;
                }
            }
            Action::Confirm => self.activate_addon_row(rows[cursor]),
            Action::Right if matches!(rows[cursor], AddonRow::Installed | AddonRow::Addon(_)) => self.activate_addon_row(rows[cursor]),
            Action::Right | Action::Left if rows[cursor] == AddonRow::Position => {}
            Action::Left | Action::Right => self.adjust_addon_row(rows[cursor], if action == Action::Left { -1 } else { 1 }),
            Action::Add => self.import_app_file(),
            Action::Back => self.addon_manager_back(),
            Action::Options => self.mode = self.settings_at(SettingRow::AddonManager),
            _ => {}
        }
    }

    pub(super) fn draw_addon_manager(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::AddonManager(manager) = &self.mode else { return };
        let view = manager.view.clone();
        if let AddonView::Move { id, .. } = &view {
            return self.draw_addon_move(ui, id, screen, s);
        }
        let addon = self.managed_addon(&view).map(|(_, m)| m);
        let rows = self.addon_rows(&view);
        let cursor = manager.cursor.min(rows.len() - 1);

        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(110));
        let width = (900.0 * s).min(screen.width() - 40.0 * s);
        let panel = Rect::from_min_max(pos2(screen.center().x - width / 2.0, screen.top() + 84.0 * s), pos2(screen.center().x + width / 2.0, screen.bottom() - 100.0 * s));
        painter.rect_filled(panel, (8.0 * s) as u8, Color32::from_rgba_unmultiplied(14, 20, 36, 245));
        let x = panel.left() + 40.0 * s;
        painter.text(pos2(x, panel.top() + 32.0 * s), Align2::LEFT_TOP, tr!("Addons Manager"), FontId::proportional(30.0 * s), Color32::WHITE);
        let (crumb, note) = match (&view, addon) {
            (AddonView::Menu, _) => (None, tr!("{count} installed", count = self.addons.installed.len())),
            (AddonView::Installed, _) => (Some(tr!("Installed addons").to_owned()), tr!("Choose an addon to see its options").to_owned()),
            (AddonView::Addon(_), Some(m)) => {
                let by = m.author.as_ref().map(|a| tr!("by {author}", author = a)).unwrap_or_default();
                (Some(format!("{} › {}", tr!("Installed addons"), m.name)), [m.version.as_str(), by.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(" · "))
            }
            (AddonView::Addon(_) | AddonView::Move { .. }, _) => (None, String::new()),
        };
        let mut y = panel.top() + 78.0 * s;
        if let Some(crumb) = crumb {
            painter.text(pos2(x, y), Align2::LEFT_TOP, crumb, FontId::proportional(22.0 * s), ACCENT);
            y += 32.0 * s;
        }
        painter.text(pos2(x, y), Align2::LEFT_TOP, note, FontId::proportional(16.0 * s), Color32::from_gray(140));

        let view_rect = Rect::from_min_max(pos2(panel.left() + 20.0 * s, y + 40.0 * s), panel.max - vec2(20.0 * s, 20.0 * s));
        let row_h = 72.0 * s;
        let scroll = follow_scroll(ui, Id::new(("addon-manager-scroll", rows.len())), cursor as f32 * row_h, row_h, rows.len() as f32 * row_h, view_rect.height());
        let painter = painter.with_clip_rect(view_rect);
        let mut hovered = None;
        let mut clicked = None;
        let mut adjusted = None;
        for (i, &row) in rows.iter().enumerate() {
            let r = Rect::from_min_size(pos2(view_rect.left(), view_rect.top() - scroll + i as f32 * row_h), vec2(view_rect.width(), row_h - 6.0 * s));
            if !view_rect.intersects(r) {
                continue;
            }
            let active = i == cursor;
            if active {
                painter.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(22));
                painter.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
            }
            let mut text_x = r.left() + 30.0 * s;
            if let AddonRow::Addon(a) = row
                && let Some(icon) = self.addons.installed.get(a).and_then(|m| Some(appstore::addon_dir(&m.id).join(m.icon.as_ref()?)))
            {
                let side = r.height() - 16.0 * s;
                let icon_rect = Rect::from_min_size(pos2(text_x, r.center().y - side / 2.0), vec2(side, side));
                if view_rect.contains_rect(icon_rect) {
                    egui::Image::new(file_uri(&icon)).paint_at(ui, icon_rect);
                }
                text_x += side + 16.0 * s;
            }
            let (label, sub, value) = self.addon_row_content(row, addon);
            let label_color = if row == AddonRow::Uninstall && active { PAD_B } else { Color32::WHITE };
            let right = r.right() - 24.0 * s;
            let font = FontId::proportional(17.0 * s);
            let mut arrows = None;
            let value_left = match value {
                RowValue::Choice(text) => {
                    let galley = painter.layout_no_wrap(text, font, Color32::WHITE);
                    let w = galley.size().x;
                    arrows = Some(value_arrows(&painter, r, right, w, active, s));
                    painter.galley(pos2(right - 26.0 * s - w, r.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
                    right - 70.0 * s - w
                }
                RowValue::Action(text) => painter.text(pos2(right, r.center().y), Align2::RIGHT_CENTER, text, font, Color32::from_gray(200)).left(),
                RowValue::Summary(text) => painter.text(pos2(right, r.center().y), Align2::RIGHT_CENTER, text, font, Color32::from_gray(200)).left(),
                _ => right,
            };
            let text_clip = painter.with_clip_rect(Rect::from_min_max(r.min, pos2(value_left - 20.0 * s, r.max.y)).intersect(view_rect));
            text_clip.text(pos2(text_x, r.center().y - 2.0 * s), Align2::LEFT_BOTTOM, label, FontId::proportional(21.0 * s), label_color);
            text_clip.text(pos2(text_x, r.center().y + 2.0 * s), Align2::LEFT_TOP, sub, FontId::proportional(15.0 * s), Color32::from_gray(140));

            let visible = r.intersect(view_rect);
            if let Some([less, more]) = arrows {
                for (rect, delta, n) in [(less, -1, 0), (more, 1, 1)] {
                    if ui.interact(rect.intersect(visible), Id::new(("addon-manager-arrow", i, n)), Sense::click()).clicked() {
                        adjusted = Some((row, delta));
                    }
                }
            }
            let resp = ui.interact(visible, Id::new(("addon-manager-row", i)), Sense::click());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                hovered = Some(i);
            }
            if resp.clicked() && adjusted.is_none() {
                clicked = Some((i, row));
            }
        }

        if let Mode::AddonManager(manager) = &mut self.mode
            && let Some(i) = hovered.or(clicked.map(|(i, _)| i))
        {
            manager.cursor = i;
        }
        if let Some((row, delta)) = adjusted {
            self.adjust_addon_row(row, delta);
        } else if let Some((_, row)) = clicked {
            self.activate_addon_row(row);
        }
    }
}

impl App {
    /// The move screen: the manager steps aside so the widget can be seen where it is drawn.
    fn draw_addon_move(&mut self, ui: &Ui, id: &str, screen: Rect, s: f32) {
        let Some(addon) = self.addons.get(id).cloned() else { return };
        let Some((_, status)) = self.addons.widgets(&self.settings, Placement::Screen).into_iter().find(|(m, _)| m.id == id) else { return };
        let area = self.screen_widget_rect(&addon, &status, screen, s);
        let painter = ui.painter();
        painter.rect_stroke(area.expand(6.0 * s), (6.0 * s) as u8, Stroke::new(2.0 * s, ACCENT), StrokeKind::Outside);

        let size = self.settings.addons.get(id).map_or(100, |a| a.size);
        let text = tr!("Move {name} with the arrows or drag it · the mouse wheel resizes it ({size} %)", name = addon.name, size = size);
        let banner = Rect::from_center_size(pos2(screen.center().x, screen.top() + 140.0 * s), vec2((900.0 * s).min(screen.width() - 40.0 * s), 64.0 * s));
        let banner = if banner.intersects(area) { Rect::from_center_size(pos2(banner.center().x, screen.bottom() - 220.0 * s), banner.size()) } else { banner };
        painter.rect_filled(banner, (8.0 * s) as u8, Color32::from_rgba_unmultiplied(14, 20, 36, 235));
        painter.text(banner.center(), Align2::CENTER_CENTER, text, FontId::proportional(20.0 * s), Color32::WHITE);

        let drag = ui.interact(area, Id::new(("addon-move", id)), Sense::drag());
        let delta = drag.drag_delta();
        if delta != Vec2::ZERO {
            self.move_addon(id, vec2(delta.x / screen.width(), delta.y / screen.height()));
        }
        let wheel = wheel_lines(ui);
        if wheel.abs() >= 0.5 && ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| area.contains(p)) {
            self.resize_addon(id, wheel.round().clamp(-5.0, 5.0) as i32 * SIZE_STEP);
        }
    }
}
