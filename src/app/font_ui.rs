//! Imported fonts: loading them into egui, the title font setting and the font manager.

use super::*;

impl App {
    pub(super) fn apply_fonts(&mut self) {
        let (loaded, warnings) = fonts::install(&self.ctx, &self.settings.fonts.title);
        self.fonts.retain(|f| loaded.contains(f));
        for w in warnings {
            self.notify(w);
        }
        let title = self.settings.fonts.title.clone();
        let missing = !title.is_empty() && !loaded.contains(&title);
        self.fonts_pending = Some(loaded);
        if missing {
            self.notify(tr!("Font {font} is gone; game titles use the default font", font = title));
            self.settings.fonts.title.clear();
            self.save_settings();
        }
    }

    pub(super) fn set_title_font(&mut self, file: String) {
        self.settings.fonts.title = file;
        self.save_settings();
        self.apply_fonts();
    }

    pub(super) fn game_font(&self, game: &Game) -> egui::FontFamily {
        match &game.title_font {
            Some(file) if self.fonts.contains(file) => fonts::family(file),
            _ => fonts::title_family(),
        }
    }

    pub(super) fn game_title_font(&self, game: &Game, size: f32) -> FontId {
        FontId::new(size, self.game_font(game))
    }

    pub(super) fn import_fonts(&mut self) {
        let dialog = rfd::FileDialog::new().set_title(tr!("Select fonts to import")).add_filter(tr!("Fonts"), fonts::EXTS);
        self.ask_paths(dialog, Pick::Files, Self::import_font_files);
    }

    pub(super) fn import_font_files(&mut self, paths: Vec<PathBuf>) {
        let mut imported = Vec::new();
        for path in paths {
            match fonts::import(&path) {
                Ok(name) => imported.push(name),
                Err(e) => self.notify(tr!("Could not import font: {error}", error = format!("{e:#}"))),
            }
        }
        if imported.is_empty() {
            return;
        }
        self.apply_fonts();
        match imported.as_slice() {
            [one] if self.fonts_pending.as_ref().is_some_and(|f| f.contains(one)) => {
                self.set_title_font(one.clone());
                self.notify(tr!("Imported {font}; game titles now use it", font = fonts::display_name(one)));
            }
            _ => self.notify(tr_n!(imported.len(), "Imported {n} font; choose one under Game title font", "Imported {n} fonts; choose one under Game title font")),
        }
    }

    pub(super) fn activate_font_row(&mut self) {
        let Mode::FontManager { cursor, armed } = &mut self.mode else { return };
        let Some(file) = self.fonts.get(*cursor).cloned() else {
            return self.import_fonts();
        };
        if !*armed {
            *armed = true;
            return;
        }
        let cursor = *cursor;
        self.delete_font(&file);
        self.mode = Mode::FontManager { cursor: cursor.min(self.fonts.len()), armed: false };
    }

    pub(super) fn font_usage(&self, file: &str) -> String {
        let games = self.library.games.iter().filter(|g| g.title_font.as_deref() == Some(file)).count();
        let mut uses = Vec::new();
        if self.settings.fonts.title == file {
            uses.push(tr!("Default for game titles").to_owned());
        }
        if games > 0 {
            uses.push(tr_n!(games, "{n} game", "{n} games"));
        }
        if uses.is_empty() { tr!("Not used · {file}", file = file) } else { uses.join(" · ") }
    }

    pub(super) fn delete_font(&mut self, file: &str) {
        if let Err(e) = fonts::remove(file) {
            return self.notify(tr!("Could not delete font: {error}", error = format!("{e:#}")));
        }
        if self.settings.fonts.title == file {
            self.settings.fonts.title.clear();
            self.save_settings();
        }
        let mut games_changed = false;
        for game in &mut self.library.games {
            if game.title_font.as_deref() == Some(file) {
                game.title_font = None;
                games_changed = true;
            }
        }
        if games_changed {
            self.save();
        }
        self.apply_fonts();
        self.notify(tr!("Deleted {name}", name = fonts::display_name(file)));
    }

    pub(super) fn draw_font_manager(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::FontManager { cursor, armed } = self.mode else { return };
        let count = self.fonts.len() + 1;
        let cursor = cursor.min(count - 1);
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(80));
        let panel = Rect::from_min_max(screen.min, pos2(screen.left() + (640.0 * s).min(screen.width()), screen.bottom()));
        painter.rect_filled(panel, 0, Color32::from_rgba_unmultiplied(10, 16, 32, 240));
        let x = panel.left() + 40.0 * s;
        painter.text(pos2(x, panel.top() + 110.0 * s), Align2::LEFT_TOP, tr!("Fonts"), FontId::proportional(30.0 * s), Color32::WHITE);
        let note = tr!("{count} installed · delete to remove it from every game", count = self.fonts.len());
        painter.text(pos2(x, panel.top() + 150.0 * s), Align2::LEFT_TOP, note, FontId::proportional(16.0 * s), Color32::from_gray(140));

        let row_h = 84.0 * s;
        let step = row_h + 6.0 * s;
        let top = panel.top() + 200.0 * s;
        let fits = (((panel.bottom() - 90.0 * s - top) / step).floor() as usize).max(1);
        let first = (cursor + 1).saturating_sub(fits);
        let mut new_cursor = cursor;
        let mut clicked = false;
        for (slot, i) in (first..count.min(first + fits)).enumerate() {
            let row = Rect::from_min_size(pos2(panel.left(), top + slot as f32 * step), vec2(panel.width(), row_h));
            let resp = ui.interact(row, Id::new(("font-row", i)), Sense::click());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                new_cursor = i;
            }
            if resp.clicked() {
                (new_cursor, clicked) = (i, true);
            }
            let active = i == cursor;
            if active {
                painter.rect_filled(row, 0, Color32::from_white_alpha(22));
                painter.rect_filled(Rect::from_min_size(row.min, vec2(5.0 * s, row.height())), 0, ACCENT);
            }
            let color = if active { Color32::WHITE } else { Color32::from_gray(180) };
            let right = row.right() - 40.0 * s;
            let Some(file) = self.fonts.get(i) else {
                painter.text(pos2(x, row.center().y - 4.0 * s), Align2::LEFT_BOTTOM, tr!("Import fonts…"), FontId::proportional(24.0 * s), color);
                painter.text(pos2(x, row.center().y + 4.0 * s), Align2::LEFT_TOP, tr!(".ttf, .otf or .ttc files"), FontId::proportional(16.0 * s), Color32::from_gray(140));
                continue;
            };
            let action = match (active, armed) {
                (true, true) => Some((tr!("Press again to delete"), PAD_B)),
                (true, false) => Some((tr!("Delete"), Color32::from_gray(200))),
                _ => None,
            };
            let mut text_right = right;
            if let Some((text, c)) = action {
                text_right = painter.text(pos2(right, row.center().y), Align2::RIGHT_CENTER, text, FontId::proportional(17.0 * s), c).left() - 20.0 * s;
            }
            let clip = painter.with_clip_rect(Rect::from_min_max(row.min, pos2(text_right, row.max.y)));
            clip.text(pos2(x, row.center().y - 4.0 * s), Align2::LEFT_BOTTOM, fonts::display_name(file), fonts::of(file, 26.0 * s), color);
            clip.text(pos2(x, row.center().y + 4.0 * s), Align2::LEFT_TOP, self.font_usage(file), FontId::proportional(16.0 * s), Color32::from_gray(140));
        }
        if let Mode::FontManager { cursor, armed } = &mut self.mode
            && new_cursor != *cursor
        {
            (*cursor, *armed) = (new_cursor, false);
        }
        if clicked {
            self.activate_font_row();
        }
    }
}

impl App {
    pub(super) fn handle_font_manager(&mut self, action: Action) {
        let Mode::FontManager { cursor, armed } = &mut self.mode else { return };
        let last = self.fonts.len();
        match action {
            Action::Up => (*cursor, *armed) = (cursor.saturating_sub(1), false),
            Action::Down => (*cursor, *armed) = ((*cursor + 1).min(last), false),
            Action::Confirm => self.activate_font_row(),
            Action::Add => self.import_fonts(),
            Action::Back if *armed => *armed = false,
            Action::Back | Action::Options => self.mode = self.settings_at(SettingRow::FontManage),
            _ => {}
        }
    }
}
