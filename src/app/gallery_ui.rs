//! A game's image gallery: choosing between importing a new image and reusing one the game
//! already has, the picker for reusing one, and removing images nothing shows.

use super::*;

const PICK_COLUMNS: usize = 4;

impl App {
    /// Asks whether to import a new image for `slot` or reuse one from the gallery; `back` is the
    /// Appearance row to return to.
    pub(super) fn choose_image(&mut self, slot: ImageSlot, back: SettingRow) {
        self.mode = Mode::ImageSource { slot, back, cursor: 0 };
    }

    fn gallery_len(&self) -> usize {
        self.selected_game().map_or(0, |g| g.gallery.len())
    }

    pub(super) fn handle_image_source(&mut self, action: Action) {
        let has_gallery = self.gallery_len() > 0;
        let Mode::ImageSource { slot, back, cursor } = &mut self.mode else { return };
        let (slot, back) = (*slot, *back);
        match action {
            Action::Left | Action::Right | Action::Up | Action::Down if has_gallery => *cursor = 1 - *cursor,
            Action::Confirm if *cursor == 0 => {
                self.mode = self.settings_at(back);
                self.import_image(slot, Some(back));
            }
            Action::Confirm => {
                let current = self.selected_game().and_then(|g| g.gallery.iter().position(|p| Some(p.as_path()) == g.image(slot)));
                self.mode = Mode::GalleryPick { slot, back, cursor: current.unwrap_or(0) };
            }
            Action::Back => self.mode = self.settings_at(back),
            _ => {}
        }
    }

    pub(super) fn handle_gallery_pick(&mut self, action: Action) {
        let count = self.gallery_len();
        let Mode::GalleryPick { slot, back, cursor } = &mut self.mode else { return };
        let (slot, back) = (*slot, *back);
        let last = count.saturating_sub(1);
        match action {
            Action::Left => *cursor = cursor.saturating_sub(1),
            Action::Right => *cursor = (*cursor + 1).min(last),
            Action::Up => *cursor = cursor.saturating_sub(PICK_COLUMNS),
            Action::Down => *cursor = (*cursor + PICK_COLUMNS).min(last),
            Action::Confirm => {
                let i = *cursor;
                self.use_gallery_image(slot, back, i);
            }
            Action::Back => self.mode = Mode::ImageSource { slot, back, cursor: 1 },
            _ => {}
        }
    }

    fn use_gallery_image(&mut self, slot: ImageSlot, back: SettingRow, i: usize) {
        let Some(image) = self.selected_game().and_then(|g| g.gallery.get(i).cloned()) else { return };
        self.library.set_image(self.selected, slot, image);
        self.save();
        self.mode = self.settings_at(back);
    }

    pub(super) fn remove_gallery_image(&mut self, i: usize) {
        let Some(image) = self.selected_game().and_then(|g| g.gallery.get(i).cloned()) else { return };
        if !self.library.remove_from_gallery(self.selected, &image) {
            return self.notify(tr!("This image is in use. Pick another image there first"));
        }
        self.save();
        let left = self.gallery_len();
        self.mode = self.settings_at(if left == 0 { SettingRow::GalleryEmpty } else { SettingRow::GalleryImage(i.min(left - 1)) });
        self.notify(tr!("Image deleted"));
    }

    pub(super) fn draw_image_source(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::ImageSource { slot, cursor, .. } = self.mode else { return };
        let count = self.gallery_len();
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(160));
        let dialog = Rect::from_center_size(screen.center(), vec2(680.0 * s, 260.0 * s));
        painter.rect_filled(dialog, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        let title = match slot {
            ImageSlot::Cover(style) => tr!("Cover image · {style}", style = style.label()),
            ImageSlot::Asset(_) => slot_label(slot).to_owned(),
        };
        painter.text(dialog.center_top() + vec2(0.0, 50.0 * s), Align2::CENTER_CENTER, title, FontId::proportional(28.0 * s), Color32::WHITE);
        let detail = tr!("Import a new file, or reuse an image this game already has");
        painter.text(dialog.center_top() + vec2(0.0, 95.0 * s), Align2::CENTER_CENTER, detail, FontId::proportional(17.0 * s), Color32::from_gray(170));

        let mut clicked = None;
        let gallery = tr!("Use from gallery ({count})", count = count);
        for (i, label) in [tr!("Import new").to_owned(), gallery].into_iter().enumerate() {
            let enabled = i == 0 || count > 0;
            let center = dialog.center_bottom() + vec2((i as f32 - 0.5) * 290.0 * s, -60.0 * s);
            let btn = Rect::from_center_size(center, vec2(260.0 * s, 54.0 * s));
            let focused = enabled && cursor == i;
            painter.rect_filled(btn, (4.0 * s) as u8, if focused { Color32::WHITE } else { Color32::from_white_alpha(25) });
            let text = match (focused, enabled) {
                (true, _) => Color32::BLACK,
                (false, true) => Color32::WHITE,
                (false, false) => Color32::from_gray(110),
            };
            painter.text(btn.center(), Align2::CENTER_CENTER, label, FontId::proportional(22.0 * s), text);
            if enabled && ui.interact(btn, Id::new(("image-source", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }
        if let Some(i) = clicked
            && let Mode::ImageSource { cursor, .. } = &mut self.mode
        {
            *cursor = i;
            self.handle(ui.ctx(), Action::Confirm);
        }
    }

    pub(super) fn draw_gallery_pick(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::GalleryPick { slot, back, cursor } = self.mode else { return };
        let Some(game) = self.selected_game() else { return };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(190));
        let panel = Rect::from_center_size(screen.center(), vec2((1100.0 * s).min(screen.width() - 40.0 * s), screen.height() - 150.0 * s));
        painter.rect_filled(panel, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        let title = tr!("Use from gallery · {slot}", slot = match slot {
            ImageSlot::Cover(style) => style.label(),
            ImageSlot::Asset(_) => slot_label(slot),
        });
        painter.text(panel.left_top() + vec2(40.0 * s, 32.0 * s), Align2::LEFT_TOP, title, FontId::proportional(30.0 * s), Color32::WHITE);

        let view = Rect::from_min_max(panel.left_top() + vec2(30.0 * s, 96.0 * s), panel.right_bottom() - vec2(30.0 * s, 20.0 * s));
        let gap = 16.0 * s;
        let cell_w = (view.width() - gap * (PICK_COLUMNS - 1) as f32) / PICK_COLUMNS as f32;
        let image_h = cell_w * 0.75;
        let cell_h = image_h + 56.0 * s;
        let rows = game.gallery.len().div_ceil(PICK_COLUMNS);
        let row_of = |i: usize| (i / PICK_COLUMNS) as f32 * (cell_h + gap);
        let scroll = follow_scroll(ui, Id::new("gallery-pick-scroll"), row_of(cursor), cell_h, rows as f32 * (cell_h + gap), view.height());
        let clip = painter.with_clip_rect(view);
        let shown = game.image(slot);
        let mut chosen = None;
        let mut hovered = None;
        for (i, image) in game.gallery.iter().enumerate() {
            let x = view.left() + (i % PICK_COLUMNS) as f32 * (cell_w + gap);
            let cell = Rect::from_min_size(pos2(x, view.top() + row_of(i) - scroll), vec2(cell_w, cell_h));
            if !view.intersects(cell) {
                continue;
            }
            let active = i == cursor;
            if active {
                clip.rect_filled(cell.expand(6.0 * s), (6.0 * s) as u8, Color32::from_white_alpha(22));
                clip.rect_stroke(cell.expand(6.0 * s), (6.0 * s) as u8, Stroke::new(3.0 * s, ACCENT), StrokeKind::Inside);
            }
            let picture = Rect::from_min_size(cell.min, vec2(cell_w, image_h));
            clip.rect_filled(picture, (4.0 * s) as u8, Color32::from_white_alpha(12));
            paint_image_contained(ui.ctx(), &clip, image, picture);
            let uses: Vec<_> = game.image_uses(image).into_iter().map(slot_label).collect();
            let caption = if Some(image.as_path()) == shown {
                tr!("Shown here now").to_owned()
            } else if uses.is_empty() {
                tr!("Not used").to_owned()
            } else {
                uses.join(" · ")
            };
            let color = if active { Color32::WHITE } else { Color32::from_gray(170) };
            let caption = clip.layout(caption, FontId::proportional(15.0 * s), color, cell_w);
            clip.galley(pos2(cell.left(), picture.bottom() + 8.0 * s), caption, color);

            let resp = ui.interact(cell.intersect(view), Id::new(("gallery-pick", i)), Sense::click());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                hovered = Some(i);
            }
            if resp.clicked() {
                chosen = Some(i);
            }
        }
        if let Some(i) = hovered {
            self.mode = Mode::GalleryPick { slot, back, cursor: i };
        }
        if let Some(i) = chosen {
            self.use_gallery_image(slot, back, i);
        }
    }
}
