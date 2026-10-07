//! Button hints: the controller/keyboard button glyphs, their skins, and the hint rows that use them.

use super::*;
use egui::ImageSource;
use egui::load::Bytes;
use std::borrow::Cow;

/// What is printed on a drawn button.
pub(super) enum Mark {
    Text(String),
    Cross,
    Circle,
    Square,
    Triangle,
}

pub(super) struct ButtonSkin {
    pub(super) mark: Mark,
    pub(super) fill: Color32,
    pub(super) text: Color32,
    pub(super) shape: ButtonShape,
    /// Drawn instead of the button once loaded, at this size relative to the button height.
    pub(super) image: Option<(ImageSource<'static>, f32)>,
}

impl ButtonSkin {
    pub(super) fn plain(text: &str, fill: Color32) -> Self {
        ButtonSkin { mark: Mark::Text(text.to_owned()), fill, text: Color32::WHITE, shape: ButtonShape::Circle, image: None }
    }
}

/// The built-in button pictures leave a margin around the button; drawn a bit larger they match drawn buttons.
const PROMPT_IMAGE_SCALE: f32 = 1.25;

macro_rules! prompt_image {
    ($folder:literal, $name:literal) => {
        ImageSource::Bytes {
            uri: Cow::Borrowed(concat!("bytes://button-prompts/", $name, ".png")),
            bytes: Bytes::Static(include_bytes!(concat!("../../assets/buttons_prompt_images/", $folder, "/Double/", $name, ".png"))),
        }
    };
}

macro_rules! xbox {
    ($name:literal) => {
        prompt_image!("Xbox_Series", $name)
    };
}

macro_rules! playstation {
    ($name:literal) => {
        prompt_image!("PlayStation_Series", $name)
    };
}

/// The built-in picture of a controller button; `None` shows the whole D-pad.
pub(super) fn prompt_image(family: PadFamily, button: Option<PadButton>) -> Option<ImageSource<'static>> {
    use PadButton as B;
    Some(match family {
        PadFamily::Xbox => match button {
            None => xbox!("xbox_dpad"),
            Some(B::South) => xbox!("xbox_button_color_a"),
            Some(B::East) => xbox!("xbox_button_color_b"),
            Some(B::West) => xbox!("xbox_button_color_x"),
            Some(B::North) => xbox!("xbox_button_color_y"),
            Some(B::LeftBumper) => xbox!("xbox_lb"),
            Some(B::RightBumper) => xbox!("xbox_rb"),
            Some(B::LeftTrigger) => xbox!("xbox_lt"),
            Some(B::RightTrigger) => xbox!("xbox_rt"),
            Some(B::LeftStick) => xbox!("xbox_ls"),
            Some(B::RightStick) => xbox!("xbox_rs"),
            Some(B::View) => xbox!("xbox_button_view"),
            Some(B::Menu) => xbox!("xbox_button_menu"),
            Some(B::Guide) => xbox!("xbox_guide"),
            Some(B::DPadUp) => xbox!("xbox_dpad_up"),
            Some(B::DPadDown) => xbox!("xbox_dpad_down"),
            Some(B::DPadLeft) => xbox!("xbox_dpad_left"),
            Some(B::DPadRight) => xbox!("xbox_dpad_right"),
        },
        PadFamily::PlayStation => match button {
            None => playstation!("playstation_dpad"),
            Some(B::South) => playstation!("playstation_button_color_cross"),
            Some(B::East) => playstation!("playstation_button_color_circle"),
            Some(B::West) => playstation!("playstation_button_color_square"),
            Some(B::North) => playstation!("playstation_button_color_triangle"),
            Some(B::LeftBumper) => playstation!("playstation_trigger_l1"),
            Some(B::RightBumper) => playstation!("playstation_trigger_r1"),
            Some(B::LeftTrigger) => playstation!("playstation_trigger_l2"),
            Some(B::RightTrigger) => playstation!("playstation_trigger_r2"),
            Some(B::LeftStick) => playstation!("playstation_button_l3"),
            Some(B::RightStick) => playstation!("playstation_button_r3"),
            Some(B::View) => playstation!("playstation4_button_share"),
            Some(B::Menu) => playstation!("playstation4_button_options"),
            Some(B::Guide) => return None,
            Some(B::DPadUp) => playstation!("playstation_dpad_up"),
            Some(B::DPadDown) => playstation!("playstation_dpad_down"),
            Some(B::DPadLeft) => playstation!("playstation_dpad_left"),
            Some(B::DPadRight) => playstation!("playstation_dpad_right"),
        },
    })
}

/// The drawn button before the theme changes it: what it shows, its fill and its letter color.
pub(super) fn drawn_default(family: PadFamily, button: PadButton) -> (Mark, Color32, Color32) {
    const PS_FILL: Color32 = Color32::from_gray(45);
    let symbol = |mark, color| (mark, PS_FILL, color);
    match (family, button) {
        (PadFamily::PlayStation, PadButton::South) => symbol(Mark::Cross, Color32::from_rgb(0x7c, 0xa8, 0xf0)),
        (PadFamily::PlayStation, PadButton::East) => symbol(Mark::Circle, Color32::from_rgb(0xf0, 0x6e, 0x72)),
        (PadFamily::PlayStation, PadButton::West) => symbol(Mark::Square, Color32::from_rgb(0xe0, 0x8c, 0xd0)),
        (PadFamily::PlayStation, PadButton::North) => symbol(Mark::Triangle, Color32::from_rgb(0x3c, 0xd0, 0xa8)),
        (PadFamily::Xbox, PadButton::South) => (Mark::Text("A".into()), PAD_A, Color32::WHITE),
        (PadFamily::Xbox, PadButton::East) => (Mark::Text("B".into()), PAD_B, Color32::WHITE),
        (PadFamily::Xbox, PadButton::West) => (Mark::Text("X".into()), PAD_X, Color32::WHITE),
        (PadFamily::Xbox, PadButton::North) => (Mark::Text("Y".into()), PAD_Y, Color32::WHITE),
        _ => (Mark::Text(button.glyph(family).to_owned()), PAD_MENU, Color32::WHITE),
    }
}

pub(super) fn button_image(painter: &Painter, skin: &ButtonSkin, h: f32) -> Option<(egui::TextureId, Vec2)> {
    let (source, scale) = skin.image.clone()?;
    let h = h * scale;
    let Ok(egui::load::TexturePoll::Ready { texture }) = egui::Image::new(source).load_for_size(painter.ctx(), vec2(h, h)) else { return None };
    let aspect = (texture.size.x / texture.size.y.max(1.0)).clamp(0.25, 4.0);
    Some((texture.id, vec2(h * aspect, h)))
}

pub(super) fn button_width(painter: &Painter, skin: &ButtonSkin, font: &FontId, h: f32, s: f32) -> f32 {
    if let Some((_, size)) = button_image(painter, skin, h) {
        return size.x;
    }
    match &skin.mark {
        Mark::Text(text) => (painter.layout_no_wrap(text.clone(), font.clone(), skin.text).size().x + 16.0 * s).max(h),
        _ => h,
    }
}

pub(super) fn paint_button(painter: &Painter, left_center: Pos2, skin: &ButtonSkin, font: FontId, h: f32, s: f32) -> Rect {
    if let Some((texture, size)) = button_image(painter, skin, h) {
        let rect = Rect::from_min_size(pos2(left_center.x, left_center.y - size.y / 2.0), size);
        painter.image(texture, rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        return rect;
    }
    let w = button_width(painter, skin, &font, h, s);
    let button = Rect::from_min_size(pos2(left_center.x, left_center.y - h / 2.0), vec2(w, h));
    let radius = match skin.shape {
        ButtonShape::Circle => h / 2.0,
        ButtonShape::Square => h * 0.2,
    };
    painter.rect_filled(button, radius as u8, skin.fill);
    let (c, r) = (button.center(), h * 0.22);
    let stroke = Stroke::new(h * 0.08, skin.text);
    match &skin.mark {
        Mark::Text(text) => {
            let galley = painter.layout_no_wrap(text.clone(), font, skin.text);
            painter.galley(c - galley.size() / 2.0, galley, skin.text);
        }
        Mark::Cross => {
            painter.line_segment([c - vec2(r, r), c + vec2(r, r)], stroke);
            painter.line_segment([c + vec2(-r, r), c + vec2(r, -r)], stroke);
        }
        Mark::Circle => {
            painter.circle_stroke(c, r, stroke);
        }
        Mark::Square => {
            painter.rect_stroke(Rect::from_center_size(c, Vec2::splat(r * 1.8)), 0, stroke, StrokeKind::Middle);
        }
        Mark::Triangle => {
            let points = [c + vec2(0.0, -r * 1.05), c + vec2(r * 1.05, r * 0.75), c + vec2(-r * 1.05, r * 0.75)];
            painter.add(Shape::closed_line(points.to_vec(), stroke));
        }
    }
    button
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

/// Marks a controller button inside a text; [`text_with_buttons`] draws it as the button.
pub(super) fn button_token(button: PadButton) -> String {
    format!("{BUTTON_OPEN}{}{BUTTON_CLOSE}", button.key())
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
        let (key, rest) = match part.split_once(BUTTON_CLOSE) {
            Some((key, rest)) if i > 0 => (Some(key), rest),
            _ => (None, part),
        };
        if let Some(key) = key {
            x = paint_button(painter, pos2(x, y), &skin(key), font.clone(), h, s).right();
        }
        if !rest.is_empty() {
            x = painter.text(pos2(x, y), Align2::LEFT_CENTER, rest, font.clone(), color).right();
        }
    }
}

impl App {
    /// The controller whose buttons the hints show: the one picked in Settings, else the one in use.
    pub(super) fn pad_family(&self) -> PadFamily {
        self.settings.controls.prompts.or_else(|| self.input.pad_family()).unwrap_or(PadFamily::Xbox)
    }

    pub(super) fn button_skin(&self, button: PadButton) -> ButtonSkin {
        let family = self.pad_family();
        let theme = self.theme();
        let look = theme.settings.buttons.get(button);
        let (mark, fill, text) = drawn_default(family, button);
        let custom = theme.button_icon(button).map(|path| (ImageSource::Uri(file_uri(&path).into()), 1.0));
        let built_in = || (theme.settings.buttons.style() == ButtonStyle::Images).then(|| prompt_image(family, Some(button))).flatten();
        ButtonSkin {
            mark,
            fill: look.color.unwrap_or(fill),
            text: look.text_color.unwrap_or(text),
            shape: look.shape.unwrap_or_default(),
            image: custom.or_else(|| built_in().map(|image| (image, PROMPT_IMAGE_SCALE))),
        }
    }

    /// The skin for a button token in a text.
    pub(super) fn token_skin(&self, key: &str) -> ButtonSkin {
        match PadButton::from_key(key) {
            Some(button) => self.button_skin(button),
            None => ButtonSkin::plain(key, PAD_MENU),
        }
    }

    fn dpad_skin(&self) -> ButtonSkin {
        let mut skin = ButtonSkin::plain("D-pad", PAD_MENU);
        if self.theme().settings.buttons.style() == ButtonStyle::Images {
            skin.image = prompt_image(self.pad_family(), None).map(|image| (image, PROMPT_IMAGE_SCALE));
        }
        skin
    }

    pub(super) fn draw_hints(&self, ui: &Ui, screen: Rect, s: f32) -> Option<Action> {
        if let Some(hints) = self.device_sync_hints() {
            let y = self.bar_rect(BarSlot::Bottom, screen, s).center().y;
            return self.draw_hint_row(ui, &hints, screen.right() - 60.0 * s, y, s, "hint");
        }
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
            Mode::Home if self.special(self.selected) == Some(Special::Store) => &[Hint(Action::Confirm, tr!("Open Discover"))],
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
            Mode::SwapBubble { .. } | Mode::SwapTile { .. } => &[Hint(Action::Confirm, tr!("Swap here")), Hint(Action::Back, tr!("Cancel"))],
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
            Mode::AddonManager(AddonManager { view: AddonView::Move { .. }, .. }) => &[Hint(Action::Confirm, tr!("Done")), Hint(Action::Back, tr!("Cancel"))],
            Mode::AddonManager(_) => &[
                Hint(Action::Confirm, tr!("Select")),
                Hint(Action::Add, tr!("Import")),
                Hint(Action::Back, tr!("Back")),
            ],
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
            let (skin, click) = match self.input.bindings().button_for(control).filter(|_| gamepad || no_key) {
                _ if matches!(control, Control::Up | Control::Down | Control::Left | Control::Right) => {
                    if gamepad { (self.dpad_skin(), None) } else { (ButtonSkin::plain(tr!("Arrows"), KEY_FILL), None) }
                }
                Some(button) => (self.button_skin(button), Some(*action)),
                None => (ButtonSkin::plain(keyboard_key(*action), KEY_FILL), Some(*action)),
            };
            let label = painter.layout_no_wrap((*label).to_owned(), font.clone(), Color32::from_gray(220));
            let left = x - label.size().x - 10.0 * s - button_width(painter, &skin, &font, h, s);
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
            paint_button(painter, pos2(left, y), &skin, font.clone(), h, s);
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
