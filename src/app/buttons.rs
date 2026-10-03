//! Button hints: the controller/keyboard button glyphs, their skins, and the hint rows that use them.

use super::*;

pub(super) struct ButtonSkin {
    pub(super) fill: Color32,
    pub(super) text: Color32,
    pub(super) icon: Option<PathBuf>,
}

impl ButtonSkin {
    pub(super) fn plain(fill: Color32) -> Self {
        ButtonSkin { fill, text: Color32::WHITE, icon: None }
    }
}

pub(super) fn button_icon(painter: &Painter, skin: &ButtonSkin, h: f32) -> Option<(egui::TextureId, Vec2)> {
    let image = egui::Image::new(file_uri(skin.icon.as_deref()?));
    let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(painter.ctx(), vec2(h, h)) else { return None };
    let aspect = (texture.size.x / texture.size.y.max(1.0)).clamp(0.25, 4.0);
    Some((texture.id, vec2(h * aspect, h)))
}

pub(super) fn button_width(painter: &Painter, glyph: &str, skin: &ButtonSkin, font: &FontId, h: f32, s: f32) -> f32 {
    if let Some((_, size)) = button_icon(painter, skin, h) {
        return size.x;
    }
    (painter.layout_no_wrap(glyph.to_owned(), font.clone(), skin.text).size().x + 16.0 * s).max(h)
}

pub(super) fn paint_button(painter: &Painter, left_center: Pos2, glyph: &str, skin: &ButtonSkin, font: FontId, h: f32, s: f32) -> Rect {
    if let Some((texture, size)) = button_icon(painter, skin, h) {
        let rect = Rect::from_min_size(pos2(left_center.x, left_center.y - h / 2.0), size);
        painter.image(texture, rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        return rect;
    }
    let galley = painter.layout_no_wrap(glyph.to_owned(), font, skin.text);
    let w = (galley.size().x + 16.0 * s).max(h);
    let pill = Rect::from_min_size(pos2(left_center.x, left_center.y - h / 2.0), vec2(w, h));
    painter.rect_filled(pill, (h / 2.0) as u8, skin.fill);
    painter.galley(pill.center() - galley.size() / 2.0, galley, skin.text);
    pill
}

pub(super) fn glyph_color(glyph: &str) -> Color32 {
    match glyph {
        "A" => PAD_A,
        "B" => PAD_B,
        "X" => PAD_X,
        "Y" => PAD_Y,
        _ => PAD_MENU,
    }
}

pub(super) fn keyboard_key(action: Action) -> &'static str {
    match action {
        Action::Confirm => "Enter",
        Action::Back => "Esc",
        Action::Options => "Tab",
        Action::Add => "Ins",
        Action::Erase => "Bksp",
        Action::ToggleFullscreen => "F11",
        _ => "",
    }
}

pub(super) fn button_token(button: PadButton) -> String {
    format!("{BUTTON_OPEN}{}{BUTTON_CLOSE}", button.glyph())
}

pub(super) fn face_token(button: FaceButton) -> String {
    format!("{BUTTON_OPEN}{}{BUTTON_CLOSE}", button.glyph())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn text_with_buttons(painter: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32, s: f32, skin: &dyn Fn(&str) -> ButtonSkin) {
    if !text.contains(BUTTON_OPEN) {
        painter.text(pos, align, text, font, color);
        return;
    }
    let h = font.size * 1.35;
    let y = match align.y() {
        egui::Align::Min => pos.y + h / 2.0,
        egui::Align::Center => pos.y,
        egui::Align::Max => pos.y - h / 2.0,
    };
    let mut x = pos.x;
    for (i, part) in text.split(BUTTON_OPEN).enumerate() {
        let (glyph, rest) = match part.split_once(BUTTON_CLOSE) {
            Some((glyph, rest)) if i > 0 => (Some(glyph), rest),
            _ => (None, part),
        };
        if let Some(glyph) = glyph {
            x = paint_button(painter, pos2(x, y), glyph, &skin(glyph), font.clone(), h, s).right();
        }
        if !rest.is_empty() {
            x = painter.text(pos2(x, y), Align2::LEFT_CENTER, rest, font.clone(), color).right();
        }
    }
}

impl App {
    pub(super) fn button_skin(&self, glyph: &str) -> ButtonSkin {
        let Some(button) = FaceButton::from_glyph(glyph) else { return ButtonSkin::plain(glyph_color(glyph)) };
        let look = self.theme().settings.buttons.get(button);
        ButtonSkin {
            fill: look.color.unwrap_or(glyph_color(glyph)),
            text: look.text_color.unwrap_or(Color32::WHITE),
            icon: self.theme().button_icon(button),
        }
    }

    pub(super) fn draw_hints(&self, ui: &Ui, screen: Rect, s: f32) -> Option<Action> {
        let hints: &[Hint] = match self.mode {
            Mode::Home if self.empty_bubble().is_some() => &[],
            Mode::Home if self.app_running_at(self.selected) => &[
                Hint(Action::Confirm, tr!("Switch to app")),
                Hint(Action::Options, tr!("Options")),
            ],
            Mode::Home if self.game_index() == Some(self.selected) => &[
                Hint(Action::Confirm, tr!("Switch to game")),
                Hint(Action::Options, tr!("Options")),
            ],
            Mode::Home if self.selected_game().is_some() => &[
                Hint(Action::Confirm, tr!("Start")),
                Hint(Action::Add, tr!("Add game")),
                Hint(Action::Options, tr!("Options")),
            ],
            Mode::Home if self.special(self.selected) == Some(Special::Add) => &[Hint(Action::Confirm, tr!("Add game"))],
            Mode::Home if self.special(self.selected) == Some(Special::Store) => &[Hint(Action::Confirm, tr!("Open Store"))],
            Mode::Store { .. } if self.store_on_themes() => &[
                Hint(Action::Confirm, tr!("Open")),
                Hint(Action::Options, tr!("Apps")),
                Hint(Action::Add, tr!("Refresh")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Store { .. } => &[
                Hint(Action::Confirm, tr!("Get / Update / Start")),
                Hint(Action::Options, tr!("Themes")),
                Hint(Action::Add, tr!("Refresh")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::StoreTheme { .. } => &[
                Hint(Action::Confirm, tr!("Install / Use")),
                Hint(Action::Left, tr!("Pictures")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Home => &[Hint(Action::Confirm, tr!("Open"))],
            Mode::Settings { in_rows: false, .. } => &[
                Hint(Action::Confirm, tr!("Open")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Settings { .. } => &[
                Hint(Action::Confirm, tr!("Change")),
                Hint(Action::Left, tr!("Adjust")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Launching(_) => &[Hint(Action::Back, tr!("Hide"))],
            Mode::SwapBubble { target, .. } if !self.bubble_places().contains(&target) && target >= self.tile_count() => {
                &[Hint(Action::Confirm, tr!("Move here")), Hint(Action::Back, tr!("Cancel"))]
            }
            Mode::SwapBubble { .. } => &[Hint(Action::Confirm, tr!("Swap here")), Hint(Action::Back, tr!("Cancel"))],
            Mode::Options { .. } | Mode::AddMenu { .. } | Mode::ConfirmRemove { .. } => {
                &[Hint(Action::Confirm, tr!("Select")), Hint(Action::Back, tr!("Back"))]
            }
            Mode::Picker(_) => &[
                Hint(Action::Confirm, tr!("Add")),
                Hint(Action::Left, tr!("Category")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Scan(_) => &[
                Hint(Action::Confirm, tr!("Select")),
                Hint(Action::Add, tr!("All / none")),
                Hint(Action::Options, tr!("Scan again")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Prompt { .. } if self.input.has_gamepad() => &[
                Hint(Action::Confirm, tr!("Type")),
                Hint(Action::Erase, tr!("Delete")),
                Hint(Action::Add, tr!("Shift")),
                Hint(Action::Options, tr!("Save")),
                Hint(Action::Back, tr!("Cancel")),
            ],
            Mode::Prompt { .. } => &[Hint(Action::Confirm, tr!("Save")), Hint(Action::Back, tr!("Cancel"))],
            Mode::CoverEditor(_) => &[
                Hint(Action::Confirm, tr!("Accept")),
                Hint(Action::Left, tr!("Adjust")),
                Hint(Action::Add, tr!("Reset")),
                Hint(Action::Back, tr!("Cancel")),
            ],
            Mode::DiscChooser { .. } => &[Hint(Action::Confirm, tr!("Start")), Hint(Action::Back, tr!("Cancel"))],
            Mode::DiscNotFound { .. } => &[Hint(Action::Confirm, tr!("OK"))],
            Mode::Welcome { .. } => &[Hint(Action::Confirm, tr!("Select"))],
            Mode::ProfileImported { .. } => &[Hint(Action::Confirm, tr!("Select")), Hint(Action::Back, tr!("Back"))],
            Mode::Burn(_) => match &self.burning {
                Some(Burning { result: Some(Err(_)), .. }) => &[Hint(Action::Confirm, tr!("Try again"))],
                Some(b) if b.result.is_some() => &[Hint(Action::Confirm, tr!("OK"))],
                Some(_) => &[Hint(Action::Back, tr!("Hide"))],
                None => match &self.mode {
                    Mode::Burn(BurnDialog { armed: true, .. }) => &[Hint(Action::Confirm, tr!("Erase and burn")), Hint(Action::Back, tr!("Cancel"))],
                    _ => &[Hint(Action::Confirm, tr!("Burn")), Hint(Action::Back, tr!("Back"))],
                },
            },
            Mode::FontManager { armed: true, .. } => &[Hint(Action::Confirm, tr!("Delete")), Hint(Action::Back, tr!("Keep"))],
            Mode::FontManager { .. } => &[
                Hint(Action::Confirm, tr!("Select")),
                Hint(Action::Add, tr!("Import")),
                Hint(Action::Back, tr!("Back")),
            ],
        };
        let mut hints = hints.to_vec();
        if matches!(self.mode, Mode::Home) {
            if self.special_bar(Special::Add).is_some() {
                hints.retain(|h| h.0 != Action::Add);
            }
            hints.extend(self.bar_shortcuts(BarSlot::Bottom));
        }
        let y = self.bar_rect(BarSlot::Bottom, screen, s).center().y;
        self.draw_hint_row(ui, &hints, screen.right() - 60.0 * s, y, s, "hint")
    }

    pub(super) fn draw_top_shortcuts(&self, ui: &Ui, screen: Rect, s: f32) -> Option<Action> {
        if !matches!(self.mode, Mode::Home) {
            return None;
        }
        let hints = self.bar_shortcuts(BarSlot::Top);
        let mut right = screen.right() - 160.0 * s;
        if self.global_mouse.connected() {
            let text = tr!("Global Virtual Mouse");
            right -= ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(18.0 * s), PAD_A).size().x + 48.0 * s;
        }
        let y = self.bar_rect(BarSlot::Top, screen, s).center().y;
        self.draw_hint_row(ui, &hints, right, y, s, "top-shortcut")
    }

    pub(super) fn draw_hint_row(&self, ui: &Ui, hints: &[Hint], right: f32, y: f32, s: f32, id: &str) -> Option<Action> {
        let gamepad = self.input.has_gamepad();
        let font = FontId::proportional(20.0 * s);
        let h = 30.0 * s;
        let mut x = right;
        let painter = ui.painter();
        let mut clicked = None;
        for (i, Hint(action, label)) in hints.iter().enumerate().rev() {
            let control = action.control();
            let no_key = keyboard_key(*action).is_empty();
            let (glyph, skin, click) = match self.input.bindings().button_for(control).filter(|_| gamepad || no_key) {
                _ if matches!(control, Control::Up | Control::Down | Control::Left | Control::Right) => {
                    if gamepad { ("D-pad", ButtonSkin::plain(PAD_MENU), None) } else { (tr!("Arrows"), ButtonSkin::plain(KEY_FILL), None) }
                }
                Some(button) => (button.glyph(), self.button_skin(button.glyph()), Some(*action)),
                None => (keyboard_key(*action), ButtonSkin::plain(KEY_FILL), Some(*action)),
            };
            let label = painter.layout_no_wrap((*label).to_owned(), font.clone(), Color32::from_gray(220));
            let left = x - label.size().x - 10.0 * s - button_width(painter, glyph, &skin, &font, h, s);
            let area = Rect::from_min_max(pos2(left, y - h / 2.0), pos2(x, y + h / 2.0)).expand(6.0 * s);
            if let Some(action) = click {
                let resp = ui.interact(area, Id::new((id, i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.hovered() {
                    painter.rect_filled(area, (h / 2.0 + 6.0 * s) as u8, Color32::from_white_alpha(18));
                }
                if resp.clicked() {
                    clicked = Some(action);
                }
            }
            paint_button(painter, pos2(left, y), glyph, &skin, font.clone(), h, s);
            painter.galley(pos2(x - label.size().x, y - label.size().y / 2.0), label, Color32::from_gray(220));
            x = left - 28.0 * s;
        }
        clicked
    }

    pub(super) fn draw_cursor(&self, ctx: &Context, s: f32) {
        let custom = self.settings.mouse.cursor_path();
        let pos = match self.input.virtual_pointer() {
            Some(pos) => pos,
            None if custom.is_some() => match ctx.pointer_hover_pos() {
                Some(pos) => pos,
                None => return,
            },
            None => return,
        };
        if custom.is_some() && ctx.output(|o| o.cursor_icon) == egui::CursorIcon::Text {
            return;
        }
        ctx.set_cursor_icon(egui::CursorIcon::None);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Debug, Id::new("cursor")));
        let h = self.settings.mouse.cursor_size * s.max(0.6);
        if let Some(path) = custom {
            let image = egui::Image::new(file_uri(&path));
            if let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(ctx, vec2(h, h)) {
                let w = h * texture.size.x / texture.size.y.max(1.0);
                let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
                painter.image(texture.id, Rect::from_min_size(pos, vec2(w, h)), uv, Color32::WHITE);
                return;
            }
        }
        let u = h / 20.0;
        let arrow = [(0.0, 0.0), (0.0, 17.0), (12.0, 12.0)].map(|(x, y)| pos + vec2(x, y) * u);
        painter.add(Shape::convex_polygon(arrow.to_vec(), Color32::WHITE, Stroke::new(1.5 * u.max(0.7), Color32::BLACK)));
    }
}
