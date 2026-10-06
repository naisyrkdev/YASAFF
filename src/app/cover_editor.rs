//! Game artwork: picking images, models and videos, and the editor that places an image on its surface.

use super::*;

pub(super) struct CoverEditor {
    pub(super) kind: AssetKind,
    pub(super) draft: CoverPlacement,
    pub(super) cursor: usize,
    pub(super) button: EditorButton,
    /// Opened from the game's Options menu rather than its box appearance page; closing returns there.
    pub(super) from_menu: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum EditorButton {
    Reset,
    Cancel,
    Accept,
}

impl EditorButton {
    pub(super) const ALL: [Self; 3] = [Self::Reset, Self::Cancel, Self::Accept];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Reset => tr!("Reset"),
            Self::Cancel => tr!("Cancel"),
            Self::Accept => tr!("Accept"),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum CoverSlider {
    Size,
    Horizontal,
    Vertical,
}

impl CoverSlider {
    pub(super) const ALL: [Self; 3] = [Self::Size, Self::Horizontal, Self::Vertical];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Size => tr!("Size"),
            Self::Horizontal => tr!("Horizontal position"),
            Self::Vertical => tr!("Vertical position"),
        }
    }

    pub(super) fn get(self, p: &CoverPlacement) -> f32 {
        let (min, max) = CoverPlacement::ZOOM_RANGE;
        match self {
            Self::Size => (p.zoom - min) / (max - min),
            Self::Horizontal => p.x,
            Self::Vertical => p.y,
        }
    }

    pub(super) fn set(self, p: &mut CoverPlacement, t: f32) {
        let (min, max) = CoverPlacement::ZOOM_RANGE;
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Size => p.zoom = snap(min + (max - min) * t),
            Self::Horizontal => p.x = snap(t),
            Self::Vertical => p.y = snap(t),
        }
    }

    pub(super) fn step(self, p: &mut CoverPlacement, delta: i32) {
        let d = 0.05 * delta as f32;
        match self {
            Self::Size => p.zoom = snap(step(p.zoom, d, CoverPlacement::ZOOM_RANGE)),
            Self::Horizontal => p.x = snap(step(p.x, d, (0.0, 1.0))),
            Self::Vertical => p.y = snap(step(p.y, d, (0.0, 1.0))),
        }
    }

    pub(super) fn value(self, p: &CoverPlacement) -> String {
        match self {
            Self::Size => format!("{:.0} %", p.zoom * 100.0),
            Self::Horizontal => position_label(p.x, tr!("Left"), tr!("Right")),
            Self::Vertical => position_label(p.y, tr!("Top"), tr!("Bottom")),
        }
    }
}

impl App {
    pub(super) fn open_cover_editor(&mut self, kind: AssetKind, from_menu: bool) {
        let Some(game) = self.selected_game() else { return };
        let draft = game.placement(kind);
        self.mode = Mode::CoverEditor(CoverEditor { kind, draft, cursor: 0, button: EditorButton::Accept, from_menu });
    }

    pub(super) fn close_cover_editor(&mut self, accept: bool) {
        let Mode::CoverEditor(editor) = &self.mode else { return };
        let (kind, from_menu) = (editor.kind, editor.from_menu);
        if accept && let Some(game) = self.library.games.get_mut(self.selected) {
            *game.placement_mut(kind) = editor.draft;
            self.save();
        }
        self.mode = if from_menu {
            Mode::Options { cursor: self.menu_index(MenuItem::AdjustCover) }
        } else {
            self.settings_at(SettingRow::GameAdjust(kind))
        };
    }

    pub(super) fn draw_cover_editor(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::CoverEditor(editor) = &self.mode else { return };
        let Some(game) = self.selected_game() else { return };
        let (kind, mut draft, mut cursor, mut button, from_menu) = (editor.kind, editor.draft, editor.cursor, editor.button, editor.from_menu);
        let before = game.placement(kind);
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(190));
        let dialog = Rect::from_center_size(screen.center() - vec2(0.0, 20.0 * s), vec2((1100.0 * s).min(screen.width() - 40.0 * s), 820.0 * s));
        painter.rect_filled(dialog, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        let title = match kind {
            AssetKind::Spine => tr!("Spine image size and position · {name}", name = game.name),
            AssetKind::Top => tr!("Top image size and position · {name}", name = game.name),
            AssetKind::Back => tr!("Back cover size and position · {name}", name = game.name),
            AssetKind::Side => tr!("Side image size and position · {name}", name = game.name),
            AssetKind::Bottom => tr!("Bottom image size and position · {name}", name = game.name),
            AssetKind::Booklet => tr!("Booklet image size and position · {name}", name = game.name),
            _ => tr!("Cover size and position · {name}", name = game.name),
        };
        painter.text(dialog.left_top() + vec2(40.0 * s, 32.0 * s), Align2::LEFT_TOP, title, FontId::proportional(30.0 * s), Color32::WHITE);
        if kind == AssetKind::Cover {
            let note = tr!("Kept separately for this tile style: {style}", style = self.theme().settings.tile_style.label());
            painter.text(dialog.left_top() + vec2(40.0 * s, 72.0 * s), Align2::LEFT_TOP, note, FontId::proportional(18.0 * s), Color32::from_gray(150));
        }

        let settings = &self.theme().settings;
        let tile_h = 330.0 * s;
        let tile = vec2(tile_h * settings.tile_aspect(), tile_h);
        let changed = draft != before;
        for (n, (label, placement)) in [(tr!("Before"), before), (tr!("After"), draft)].into_iter().enumerate() {
            let center_x = dialog.center().x + (n as f32 - 0.5) * dialog.width() / 2.0;
            let color = if n == 1 && changed { ACCENT } else { Color32::from_gray(170) };
            painter.text(pos2(center_x, dialog.top() + 110.0 * s), Align2::CENTER_TOP, label, FontId::proportional(22.0 * s), color);
            let rect = Rect::from_center_size(pos2(center_x, dialog.top() + 150.0 * s + tile_h / 2.0), tile);
            *self.library.games[self.selected].placement_mut(kind) = placement;
            self.paint_tile_art(ui, self.selected, rect, (6.0 * s) as u8, s, 0.0);
        }
        *self.library.games[self.selected].placement_mut(kind) = before;

        let mut accept = None;
        let top = dialog.top() + 520.0 * s;
        let row_h = 62.0 * s;
        for (i, slider) in CoverSlider::ALL.into_iter().enumerate() {
            let r = Rect::from_min_size(pos2(dialog.left() + 20.0 * s, top + i as f32 * row_h), vec2(dialog.width() - 40.0 * s, row_h - 6.0 * s));
            let active = i == cursor;
            if active {
                painter.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(22));
                painter.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
            }
            let text_color = if active { Color32::WHITE } else { Color32::from_gray(190) };
            painter.text(pos2(r.left() + 30.0 * s, r.center().y), Align2::LEFT_CENTER, slider.label(), FontId::proportional(21.0 * s), text_color);
            let value_right = r.right() - 30.0 * s;
            painter.text(pos2(value_right, r.center().y), Align2::RIGHT_CENTER, slider.value(&draft), FontId::proportional(19.0 * s), text_color);
            let track = Rect::from_min_max(pos2(r.left() + 300.0 * s, r.center().y - 3.0 * s), pos2(value_right - 120.0 * s, r.center().y + 3.0 * s));
            let t = slider.get(&draft).clamp(0.0, 1.0);
            let knob = pos2(track.left() + track.width() * t, track.center().y);
            let saved_x = track.left() + track.width() * slider.get(&before).clamp(0.0, 1.0);
            painter.rect_filled(track, (3.0 * s) as u8, Color32::from_white_alpha(40));
            painter.rect_filled(Rect::from_min_max(track.min, pos2(knob.x, track.max.y)), (3.0 * s) as u8, ACCENT);
            painter.line_segment([pos2(saved_x, track.top() - 9.0 * s), pos2(saved_x, track.bottom() + 9.0 * s)], Stroke::new(2.0 * s, Color32::from_gray(150)));
            painter.circle_filled(knob, 11.0 * s, Color32::WHITE);

            let resp = ui.interact(r, Id::new(("cover-editor-row", i)), Sense::hover());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                cursor = i;
            }
            let drag = ui.interact(track.expand2(vec2(14.0 * s, 20.0 * s)), Id::new(("cover-editor-slider", i)), Sense::click_and_drag());
            if let Some(pos) = drag.interact_pointer_pos().filter(|_| drag.dragged() || drag.clicked()) {
                slider.set(&mut draft, (pos.x - track.left()) / track.width());
                cursor = i;
            }
        }

        let buttons_y = dialog.bottom() - 60.0 * s;
        let on_buttons = cursor == CoverSlider::ALL.len();
        for (n, b) in EditorButton::ALL.into_iter().enumerate() {
            let center = pos2(dialog.center().x + (n as f32 - 1.0) * 230.0 * s, buttons_y);
            let btn = Rect::from_center_size(center, vec2(200.0 * s, 54.0 * s));
            let focused = on_buttons && button == b;
            let fill = match (focused, b) {
                (true, _) => Color32::WHITE,
                (false, EditorButton::Accept) => ACCENT,
                (false, _) => Color32::from_white_alpha(25),
            };
            painter.rect_filled(btn, (4.0 * s) as u8, fill);
            let text = if focused { Color32::BLACK } else { Color32::WHITE };
            painter.text(btn.center(), Align2::CENTER_CENTER, b.label(), FontId::proportional(22.0 * s), text);
            let resp = ui.interact(btn, Id::new(("cover-editor-button", n)), Sense::click());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                (cursor, button) = (CoverSlider::ALL.len(), b);
            }
            if resp.clicked() {
                match b {
                    EditorButton::Reset => draft = CoverPlacement::default(),
                    EditorButton::Cancel => accept = Some(false),
                    EditorButton::Accept => accept = Some(true),
                }
            }
        }

        self.mode = Mode::CoverEditor(CoverEditor { kind, draft, cursor, button, from_menu });
        if let Some(accept) = accept {
            self.close_cover_editor(accept);
        }
    }

    pub(super) fn pick_asset(&mut self, kind: AssetKind) {
        if let AssetKind::Model = kind {
            return self.pick_model();
        }
        if let AssetKind::Video | AssetKind::IdleVideo = kind {
            let title = if kind == AssetKind::Video { tr!("Select preview video") } else { tr!("Select idle video") };
            let dialog = rfd::FileDialog::new().set_title(title).add_filter(tr!("Videos"), video::EXTS);
            let game_id = self.selected_game().map(|g| g.id.clone()).unwrap_or_default();
            return self.ask_path(dialog, Pick::File, move |app, path| {
                let Some(i) = app.library.position(&game_id) else { return };
                match app.library.set_asset(i, kind, &path) {
                    Ok(()) => app.save(),
                    Err(e) => app.notify(tr!("Could not use the video: {error}", error = format!("{e:#}"))),
                }
            });
        }
        let title = match kind {
            AssetKind::Cover => tr!("Select cover image (square works best)"),
            AssetKind::Background => tr!("Select background image (16:9 works best)"),
            AssetKind::Spine => tr!("Select spine image (a tall or wide scan of the box spine)"),
            AssetKind::Top => tr!("Select box top image"),
            AssetKind::Back => tr!("Select back cover image"),
            AssetKind::Side => tr!("Select right side image (the edge opposite the spine)"),
            AssetKind::Bottom => tr!("Select box bottom image"),
            AssetKind::Booklet => tr!("Select instruction booklet image"),
            AssetKind::Model | AssetKind::Video | AssetKind::IdleVideo => unreachable!(),
        };
        let dialog = rfd::FileDialog::new().set_title(title).add_filter(tr!("Images"), IMAGE_EXTS);
        let game_id = self.selected_game().map(|g| g.id.clone()).unwrap_or_default();
        self.ask_path(dialog, Pick::File, move |app, path| {
            let Some(i) = app.library.position(&game_id) else { return };
            match app.library.set_asset(i, kind, &path) {
                Ok(()) => app.save(),
                Err(e) => app.notify(tr!("Could not set image: {error}", error = format!("{e:#}"))),
            }
        });
    }

    pub(super) fn pick_model(&mut self) {
        let dialog = rfd::FileDialog::new().set_title(tr!("Select a 3D model (STL, OBJ, PLY, glTF / GLB or 3MF)")).add_filter(tr!("3D models"), object3d::EXTS);
        let game_id = self.selected_game().map(|g| g.id.clone()).unwrap_or_default();
        self.ask_path(dialog, Pick::File, move |app, path| {
            let Some(i) = app.library.position(&game_id) else { return };
            match app.library.set_model(i, &path) {
                Ok(()) => app.save(),
                Err(e) => app.notify(tr!("Could not use the model: {error}", error = format!("{e:#}"))),
            }
        });
    }
}

impl App {
    pub(super) fn handle_cover_editor(&mut self, action: Action) {
        let Mode::CoverEditor(editor) = &mut self.mode else { return };
        let on_buttons = editor.cursor == CoverSlider::ALL.len();
        match action {
            Action::Up => editor.cursor = editor.cursor.saturating_sub(1),
            Action::Down => editor.cursor = (editor.cursor + 1).min(CoverSlider::ALL.len()),
            Action::Left | Action::Right => {
                let delta = if action == Action::Left { -1 } else { 1 };
                if on_buttons {
                    editor.button = cycle(&EditorButton::ALL, editor.button, delta);
                } else {
                    CoverSlider::ALL[editor.cursor].step(&mut editor.draft, delta);
                }
            }
            Action::Add => editor.draft = CoverPlacement::default(),
            Action::Confirm => match editor.button {
                EditorButton::Reset if on_buttons => editor.draft = CoverPlacement::default(),
                EditorButton::Cancel if on_buttons => self.close_cover_editor(false),
                _ => self.close_cover_editor(true),
            },
            Action::Back => self.close_cover_editor(false),
            _ => {}
        }
    }
}
