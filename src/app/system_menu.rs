//! The system options: shut down or minimize YASAFF, opened by holding Guide or Esc. In
//! fullscreen there is no window frame, so this is the way out.

use super::*;

#[derive(Clone, Copy, PartialEq)]
enum SystemOption {
    ShutDown,
    Minimize,
    Cancel,
}

impl SystemOption {
    const ALL: [Self; 3] = [Self::ShutDown, Self::Minimize, Self::Cancel];

    fn label(self) -> &'static str {
        match self {
            Self::ShutDown => tr!("Shut down"),
            Self::Minimize => tr!("Minimize"),
            Self::Cancel => tr!("Cancel"),
        }
    }
}

impl App {
    pub(super) fn handle_system_menu(&mut self, ctx: &Context, action: Action) {
        let Some(cursor) = &mut self.system_menu else { return };
        match action {
            Action::Up | Action::Left => *cursor = cursor.saturating_sub(1),
            Action::Down | Action::Right => *cursor = (*cursor + 1).min(SystemOption::ALL.len() - 1),
            Action::Confirm => {
                let option = SystemOption::ALL[*cursor];
                self.system_menu = None;
                match option {
                    SystemOption::ShutDown => ctx.send_viewport_cmd(ViewportCommand::Close),
                    SystemOption::Minimize => {
                        self.focus_guard_until = None;
                        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                    }
                    SystemOption::Cancel => {}
                }
            }
            Action::Back => self.system_menu = None,
            _ => {}
        }
    }

    /// The button that opens the system options: the one bound to Guide on a controller, else Esc.
    fn system_skin(&self) -> ButtonSkin {
        match self.input.bindings().button_for(Control::Guide).filter(|_| self.input.has_gamepad()) {
            Some(button) => self.button_skin(button),
            None => ButtonSkin::plain(keyboard_key(Action::Back), KEY_FILL),
        }
    }

    /// "Hold [button] System options" in the bottom left corner, filling up while the button is held.
    pub(super) fn draw_system_hint(&self, ui: &Ui, screen: Rect, s: f32) -> Option<Action> {
        if self.system_menu.is_some() {
            return None;
        }
        let painter = ui.painter();
        let font = FontId::proportional(20.0 * s);
        let h = 30.0 * s;
        let y = self.bar_rect(BarSlot::Bottom, screen, s).center().y;
        let color = Color32::from_gray(220);
        let skin = self.system_skin();
        let hold = painter.layout_no_wrap(tr!("Hold").to_owned(), font.clone(), color);
        let label = painter.layout_no_wrap(tr!("System options").to_owned(), font.clone(), color);
        let left = screen.left() + 60.0 * s;
        let button_left = left + hold.size().x + 10.0 * s;
        let label_left = button_left + button_width(painter, &skin, &font, h, s) + 10.0 * s;
        let area = Rect::from_min_max(pos2(left, y - h / 2.0), pos2(label_left + label.size().x, y + h / 2.0)).expand(6.0 * s);

        let resp = ui.interact(area, Id::new("system-hint"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.hovered() {
            painter.rect_filled(area, (h / 2.0 + 6.0 * s) as u8, Color32::from_white_alpha(18));
        }
        painter.galley(pos2(left, y - hold.size().y / 2.0), hold, color);
        paint_button(painter, pos2(button_left, y), &skin, font, h, s);
        painter.galley(pos2(label_left, y - label.size().y / 2.0), label, color);
        if let Some(progress) = self.input.system_hold_progress() {
            let bar = Rect::from_min_size(pos2(area.left(), area.bottom() + 2.0 * s), vec2(area.width() * progress, 3.0 * s));
            painter.rect_filled(bar, (1.5 * s) as u8, ACCENT);
        }
        resp.clicked().then_some(Action::SystemMenu)
    }

    pub(super) fn draw_system_menu(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Some(cursor) = self.system_menu else { return };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(170));
        let (row_h, gap) = (60.0 * s, 16.0 * s);
        let rows = SystemOption::ALL.len() as f32;
        let dialog = Rect::from_center_size(screen.center(), vec2(480.0 * s, 120.0 * s + rows * row_h + (rows - 1.0) * gap + 40.0 * s));
        painter.rect_filled(dialog, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        painter.text(dialog.center_top() + vec2(0.0, 60.0 * s), Align2::CENTER_CENTER, tr!("System options"), FontId::proportional(28.0 * s), Color32::WHITE);

        let mut chosen = None;
        for (i, option) in SystemOption::ALL.into_iter().enumerate() {
            let center = pos2(dialog.center().x, dialog.top() + 120.0 * s + row_h / 2.0 + i as f32 * (row_h + gap));
            let btn = Rect::from_center_size(center, vec2(360.0 * s, row_h));
            let resp = ui.interact(btn, Id::new(("system-option", i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.hovered() && resp.ctx.input(|inp| inp.pointer.delta() != Vec2::ZERO) {
                self.system_menu = Some(i);
            }
            if resp.clicked() {
                chosen = Some(i);
            }
            let focused = self.system_menu.unwrap_or(cursor) == i;
            painter.rect_filled(btn, (4.0 * s) as u8, if focused { Color32::WHITE } else { Color32::from_white_alpha(25) });
            let text = if focused { Color32::BLACK } else { Color32::WHITE };
            painter.text(btn.center(), Align2::CENTER_CENTER, option.label(), FontId::proportional(22.0 * s), text);
        }

        let hints = [Hint(Action::Confirm, tr!("Select")), Hint(Action::Back, tr!("Cancel"))];
        let y = self.bar_rect(BarSlot::Bottom, screen, s).center().y;
        let clicked = self.draw_hint_row(ui, &hints, screen.right() - 60.0 * s, y, s, "system-hint-row");
        let ctx = ui.ctx().clone();
        if let Some(i) = chosen {
            self.system_menu = Some(i);
            self.handle(&ctx, Action::Confirm);
        } else if let Some(action) = clicked {
            self.handle(&ctx, action);
        }
    }
}
