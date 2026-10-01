//! Drawing the Settings screen and moving around it.

use super::*;

impl App {
    pub(in crate::app) fn draw_settings(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::Settings { cursor, page, in_rows } = self.mode else { return };
        let rows = self.setting_rows(page);
        let sections = self.setting_sections(&rows);
        let title = match (page, self.selected_game()) {
            (Page::GameBox, Some(game)) => tr!("Appearance · {name}", name = game.name),
            (Page::GameDevice, Some(game)) => tr!("Physical Devices · {name}", name = game.name),
            (Page::GameDetails, Some(game)) => tr!("Details · {name}", name = game.name),
            (Page::ThemePictures, _) => match self.pictures_row() {
                SettingRow::ThemeOption(i) => self.theme_manifest().and_then(|m| m.options.get(i)).map(|o| o.label.trim_end_matches('…').to_owned()).unwrap_or_default(),
                _ => String::new(),
            },
            _ => tr!("Settings").to_owned(),
        };
        let cursor = cursor.min(rows.len() - 1);
        let current = section_of(&sections, cursor);
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(110));
        let width = (1180.0 * s).min(screen.width() - 40.0 * s);
        let panel = Rect::from_min_max(pos2(screen.right() - 40.0 * s - width, screen.top() + 84.0 * s), screen.max - vec2(40.0 * s, 100.0 * s));
        painter.rect_filled(panel, (8.0 * s) as u8, Color32::from_rgba_unmultiplied(14, 20, 36, 245));
        painter.text(panel.left_top() + vec2(40.0 * s, 32.0 * s), Align2::LEFT_TOP, title, FontId::proportional(30.0 * s), Color32::WHITE);

        let body_top = panel.top() + 96.0 * s;
        let nav_w = (300.0 * s).min(panel.width() * 0.35);
        let nav = Rect::from_min_max(pos2(panel.left(), body_top), pos2(panel.left() + nav_w, panel.bottom() - 16.0 * s));
        painter.vline(nav.right(), nav.y_range(), Stroke::new(1.0, Color32::from_white_alpha(20)));
        let item_h = 52.0 * s;
        let nav_scroll = follow_scroll(ui, Id::new("settings-nav-scroll"), current as f32 * item_h, item_h, sections.len() as f32 * item_h, nav.height());
        let nav_painter = painter.with_clip_rect(nav);
        let mut chosen_section = None;
        for (i, section) in sections.iter().enumerate() {
            let r = Rect::from_min_size(pos2(nav.left() + 12.0 * s, nav.top() - nav_scroll + i as f32 * item_h), vec2(nav.width() - 24.0 * s, item_h - 6.0 * s));
            if !nav.intersects(r) {
                continue;
            }
            let selected = i == current;
            let resp = ui.interact(r.intersect(nav), Id::new(("setting-section", i)), Sense::click());
            if selected {
                let fill = if in_rows { Color32::from_white_alpha(12) } else { Color32::from_white_alpha(22) };
                nav_painter.rect_filled(r, (4.0 * s) as u8, fill);
                if !in_rows {
                    nav_painter.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
                }
            } else if resp.hovered() {
                nav_painter.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(8));
            }
            let color = if selected { Color32::WHITE } else { Color32::from_gray(170) };
            let text = nav_painter.text(pos2(r.left() + 24.0 * s, r.center().y), Align2::LEFT_CENTER, section.title, FontId::proportional(20.0 * s), color);
            if section.shown {
                nav_painter.circle_filled(pos2(text.right() + 14.0 * s, r.center().y), 6.0 * s, PAD_A);
            }
            if resp.clicked() {
                chosen_section = Some(i);
            }
        }

        let section = &sections[current];
        let column = Rect::from_min_max(pos2(nav.right() + 20.0 * s, body_top), pos2(panel.right() - 20.0 * s, nav.bottom()));
        let heading = painter.text(pos2(column.left() + 20.0 * s, column.top()), Align2::LEFT_TOP, section.title, FontId::proportional(24.0 * s), ACCENT);
        if section.shown {
            painter.circle_filled(pos2(heading.right() + 16.0 * s, heading.center().y), 7.0 * s, PAD_A);
        }
        let mut list_top = column.top() + 44.0 * s;
        if let Some((text, color)) = &section.status {
            let galley = painter.layout(text.clone(), FontId::proportional(16.0 * s), *color, column.width() - 40.0 * s);
            let h = galley.size().y;
            painter.galley(pos2(column.left() + 20.0 * s, list_top - 6.0 * s), galley, *color);
            list_top += h + 8.0 * s;
        }
        let view = Rect::from_min_max(pos2(column.left(), list_top), column.max);
        let row_h = 66.0 * s;
        let row_y = |i: usize| (i - section.rows.start) as f32 * row_h;
        let scroll = follow_scroll(ui, Id::new(("settings-scroll", current)), row_y(cursor), row_h, section.rows.len() as f32 * row_h, view.height());
        let offset = view.top() - scroll;
        let painter = painter.with_clip_rect(view);

        let skin = |key: &str| self.token_skin(key);
        let mut clicked = None;
        let mut adjusted = None;
        let mut hovered = None;
        for i in section.rows.clone() {
            let row = rows[i];
            let r = Rect::from_min_size(pos2(column.left(), offset + row_y(i)), vec2(column.width(), row_h - 6.0 * s));
            if !view.intersects(r) {
                continue;
            }
            let (label, sub, value) = self.setting_row_content(row);
            let enabled = self.setting_enabled(row);
            let text_color = if enabled { Color32::WHITE } else { Color32::from_gray(110) };
            let active = in_rows && i == cursor;
            if active {
                painter.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(22));
                painter.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
            }
            let mut x = r.left() + 30.0 * s;
            if let Some(image) = self.setting_row_image(row) {
                let h = r.height() - 12.0 * s;
                let thumb = Rect::from_min_size(pos2(x, r.center().y - h / 2.0), vec2(h * 1.5, h));
                painter.rect_filled(thumb, (4.0 * s) as u8, Color32::from_white_alpha(12));
                paint_image_contained(ui.ctx(), &painter, &image, thumb);
                x = thumb.right() + 16.0 * s;
            }
            if sub.is_empty() {
                text_with_buttons(&painter, pos2(x, r.center().y), Align2::LEFT_CENTER, &label, FontId::proportional(21.0 * s), text_color, s, &skin);
            } else {
                text_with_buttons(&painter, pos2(x, r.center().y - 2.0 * s), Align2::LEFT_BOTTOM, &label, FontId::proportional(21.0 * s), text_color, s, &skin);
                painter.text(pos2(x, r.center().y + 2.0 * s), Align2::LEFT_TOP, sub, FontId::proportional(15.0 * s), Color32::from_gray(140));
            }

            let font = FontId::proportional(17.0 * s);
            let right = r.right() - 24.0 * s;
            let mut arrows = None;
            match value {
                RowValue::Toggle(on) => {
                    let pill = Rect::from_center_size(pos2(right - 46.0 * s, r.center().y), vec2(84.0 * s, 32.0 * s));
                    painter.rect_filled(pill, (16.0 * s) as u8, if on { ACCENT } else { Color32::from_white_alpha(25) });
                    painter.text(pill.center(), Align2::CENTER_CENTER, if on { tr!("On") } else { tr!("Off") }, font, Color32::WHITE);
                }
                RowValue::Action(text) => {
                    painter.text(pos2(right, r.center().y), Align2::RIGHT_CENTER, text, font, Color32::from_gray(200));
                }
                RowValue::Summary(ref text) => {
                    painter.text(pos2(right, r.center().y), Align2::RIGHT_CENTER, text, font, Color32::from_gray(200));
                }
                RowValue::Choice(text) => {
                    let galley = painter.layout_no_wrap(text, font, text_color);
                    if enabled {
                        arrows = Some(value_arrows(&painter, r, right, galley.size().x, active, s));
                    }
                    painter.galley(pos2(right - 26.0 * s - galley.size().x, r.center().y - galley.size().y / 2.0), galley, text_color);
                }
                RowValue::Color(color) | RowValue::DefaultColor(color) => {
                    let text = match value {
                        RowValue::DefaultColor(_) => tr!("Default").to_owned(),
                        _ => theme::to_hex(color),
                    };
                    let galley = painter.layout_no_wrap(text, font, Color32::WHITE);
                    let swatch = 26.0 * s;
                    let inner = galley.size().x + 10.0 * s + swatch;
                    arrows = Some(value_arrows(&painter, r, right, inner, active, s));
                    let text_x = right - 26.0 * s - galley.size().x;
                    let sw = Rect::from_center_size(pos2(text_x - 10.0 * s - swatch / 2.0, r.center().y), vec2(swatch, swatch));
                    painter.rect_filled(sw, (4.0 * s) as u8, color);
                    painter.rect_stroke(sw, (4.0 * s) as u8, Stroke::new(1.0, Color32::from_white_alpha(80)), StrokeKind::Outside);
                    painter.galley(pos2(text_x, r.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
                }
            }

            let visible = r.intersect(view);
            if let Some([less, more]) = arrows {
                for (rect, delta, n) in [(less, -1, 0), (more, 1, 1)] {
                    if ui.interact(rect.intersect(visible), Id::new(("setting-arrow", i, n)), Sense::click()).clicked() {
                        adjusted = Some((row, delta));
                    }
                }
            }
            let resp = ui.interact(visible, Id::new(("setting", i)), Sense::click());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                hovered = Some(i);
            }
            if resp.clicked() && adjusted.is_none() {
                clicked = Some(row);
            }
        }

        let mut cursor = cursor;
        let mut in_rows = in_rows;
        if let Some(i) = hovered {
            (cursor, in_rows) = (i, true);
        }
        if let Some(i) = chosen_section {
            (cursor, in_rows) = (sections[i].rows.start, false);
        }
        let wheel = wheel_lines(ui);
        if wheel.abs() >= 0.5 {
            let steps = -(wheel.round().clamp(-5.0, 5.0) as isize);
            if ui.input(|inp| inp.pointer.hover_pos()).is_some_and(|p| nav.contains(p)) {
                let next = current.saturating_add_signed(steps).min(sections.len() - 1);
                (cursor, in_rows) = (sections[next].rows.start, false);
            } else {
                cursor = cursor.saturating_add_signed(steps).clamp(section.rows.start, section.rows.end - 1);
            }
        }
        self.mode = Mode::Settings { cursor, page, in_rows };
        if let Some((row, delta)) = adjusted {
            self.adjust_setting(row, delta);
        } else if let Some(row) = clicked {
            self.activate_setting(row);
        }
    }

    pub(in crate::app) fn handle_settings(&mut self, action: Action) {
        let Mode::Settings { page, .. } = self.mode else { return };
        let rows = self.setting_rows(page);
        let sections = self.setting_sections(&rows);
        let Mode::Settings { cursor, page, in_rows } = &mut self.mode else { return };
        let page = *page;
        *cursor = (*cursor).min(rows.len() - 1);
        let current = section_of(&sections, *cursor);
        let range = sections[current].rows.clone();
        let mut close = false;
        if *in_rows {
            match action {
                Action::Up => *cursor = cursor.saturating_sub(1).max(range.start),
                Action::Down => *cursor = (*cursor + 1).min(range.end - 1),
                Action::Confirm => {
                    let row = rows[*cursor];
                    self.activate_setting(row);
                }
                Action::Left | Action::Right => {
                    let row = rows[*cursor];
                    self.adjust_setting(row, if action == Action::Left { -1 } else { 1 });
                }
                Action::Back => *in_rows = false,
                Action::Options => close = true,
                _ => {}
            }
        } else {
            match action {
                Action::Up => *cursor = sections[current.saturating_sub(1)].rows.start,
                Action::Down => *cursor = sections[(current + 1).min(sections.len() - 1)].rows.start,
                Action::Confirm | Action::Right => *in_rows = true,
                Action::Back | Action::Options => close = true,
                _ => {}
            }
        }
        if close {
            self.mode = match page {
                Page::App => Mode::Home,
                Page::GameBox => Mode::Options { cursor: self.menu_index(MenuItem::Appearance) },
                Page::GameDevice => Mode::Options { cursor: self.menu_index(MenuItem::PhysicalDevices) },
                Page::GameDetails => Mode::Options { cursor: self.menu_index(MenuItem::Details) },
                Page::ThemePictures => self.settings_at(self.pictures_row()),
            }
        }
    }
}
