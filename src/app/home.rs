use super::*;
use crate::appstore::Manifest;

#[derive(Clone, Copy)]
pub(super) enum Layout {
    Row { rows: usize },
    Grid { columns: usize, rows: usize },
}

pub(super) fn navigate(layout: Layout, selected: usize, count: usize, action: Action) -> usize {
    let last = count.saturating_sub(1);
    match layout {
        Layout::Row { rows } => {
            let (col, row) = (selected / rows, selected % rows);
            match action {
                Action::Up if row > 0 => selected - 1,
                Action::Down if row + 1 < rows => (selected + 1).min(last),
                Action::Left if col > 0 => selected - rows,
                Action::Right if col < last / rows => (selected + rows).min(last),
                _ => selected,
            }
        }
        Layout::Grid { columns, rows } => {
            let per_page = columns * rows;
            let (page, i) = (selected / per_page, selected % per_page);
            let (col, row) = (i % columns, i / columns);
            let page_start = page * per_page;
            match action {
                Action::Left if col > 0 => selected - 1,
                Action::Left if page > 0 => page_start - per_page + row * columns + columns - 1,
                Action::Right if col + 1 < columns && selected < last => selected + 1,
                Action::Right if page < last / per_page => (page_start + per_page + row * columns).min(last),
                Action::Up if row > 0 => selected - columns,
                Action::Down if row + 1 < rows && page_start + (row + 1) * columns <= last => (selected + columns).min(last),
                _ => selected,
            }
        }
    }
}

pub(super) fn navigate_free(centers: &[Pos2], selected: usize, action: Action) -> usize {
    let dir = match action {
        Action::Left => vec2(-1.0, 0.0),
        Action::Right => vec2(1.0, 0.0),
        Action::Up => vec2(0.0, -1.0),
        Action::Down => vec2(0.0, 1.0),
        _ => return selected,
    };
    let Some(&from) = centers.get(selected) else { return selected };
    centers
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| {
            let d = c - from;
            let along = d.dot(dir);
            let across = (d - dir * along).length();
            (i != selected && along > 1.0 && across <= along * 2.0).then_some((i, along + across * 2.0))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(selected, |(i, _)| i)
}

pub(super) struct BubbleLayout {
    pub(super) diameters: Vec<f32>,
    pub(super) area: Rect,
    pub(super) per_page: usize,
    pub(super) centers: Vec<Pos2>,
    pub(super) radii: Vec<f32>,
}

impl BubbleLayout {
    pub(super) fn pages(&self) -> usize {
        self.centers.len().div_ceil(self.per_page).max(1)
    }
}

pub(super) fn bubble_capacity(size: f32, area: Rect, s: f32) -> usize {
    let base = size.max(BubbleSettings::SIZE_RANGE.0) * s;
    ((area.area() * 0.5 / (std::f32::consts::FRAC_PI_4 * base * base)) as usize).max(1)
}

pub(super) fn navigate_bubble_pages(centers: &[Pos2], per_page: usize, area: Rect, selected: usize, action: Action) -> usize {
    let page = selected / per_page;
    let here = page * per_page..((page + 1) * per_page).min(centers.len());
    if !here.contains(&selected) {
        return selected;
    }
    let to = here.start + navigate_free(&centers[here.clone()], selected - here.start, action);
    if to != selected {
        return to;
    }
    let page = match action {
        Action::Left if page > 0 => page - 1,
        Action::Right if here.end < centers.len() => page + 1,
        _ => return selected,
    };
    bubble_page_entry(centers, per_page, area, selected, page).unwrap_or(selected)
}

pub(super) fn bubble_page_entry(centers: &[Pos2], per_page: usize, area: Rect, from: usize, page: usize) -> Option<usize> {
    let edge = if page > from / per_page { area.left() } else { area.right() };
    let aim = pos2(edge, centers.get(from)?.y);
    (page * per_page..((page + 1) * per_page).min(centers.len())).min_by(|&a, &b| centers[a].distance(aim).total_cmp(&centers[b].distance(aim)))
}

pub(super) fn pack_bubbles(diameters: &[f32], area: Rect) -> (Vec<Pos2>, Vec<f32>) {
    let n = diameters.len();
    if n == 0 || !area.is_positive() {
        return (Vec::new(), Vec::new());
    }
    let half = area.size() / 2.0;
    let total: f32 = diameters.iter().map(|d| d * d).sum::<f32>() * std::f32::consts::FRAC_PI_4;
    let biggest = diameters.iter().copied().fold(1.0, f32::max);
    let mut scale = (area.area() * 0.6 / total).sqrt().min(1.0).min(area.height() / biggest).min(area.width() / biggest);
    let aspect = (area.width() / area.height()).max(0.2);
    let mut best = (Vec::new(), Vec::new());
    for _ in 0..10 {
        let radii: Vec<f32> = diameters.iter().map(|d| d * scale / 2.0).collect();
        let mean = radii.iter().sum::<f32>() / n as f32;
        let mut pos: Vec<Vec2> = (0..n)
            .map(|i| {
                let (sin, cos) = (i as f32 * 2.399_963).sin_cos();
                vec2(cos * aspect.sqrt(), sin / aspect.sqrt()) * (i as f32 + 0.5).sqrt() * mean * 1.9
            })
            .collect();
        let steps = 320;
        for step in 0..steps {
            for i in 0..n {
                for j in i + 1..n {
                    let d = pos[j] - pos[i];
                    let dist = d.length();
                    let want = (radii[i] + radii[j]) * 1.04;
                    if dist < want {
                        let dir = if dist > 1e-3 { d / dist } else { Vec2::angled(i as f32 + j as f32) };
                        let push = dir * (want - dist) / 2.0;
                        pos[i] -= push;
                        pos[j] += push;
                    }
                }
            }
            let pull = if step < steps - 80 { 0.012 } else { 0.0 };
            for (p, r) in pos.iter_mut().zip(&radii) {
                p.x -= p.x * pull;
                p.y -= p.y * pull * aspect;
                p.x = p.x.clamp(-(half.x - r).max(0.0), (half.x - r).max(0.0));
                p.y = p.y.clamp(-(half.y - r).max(0.0), (half.y - r).max(0.0));
            }
        }
        let worst = (0..n)
            .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
            .map(|(i, j)| (radii[i] + radii[j] - (pos[j] - pos[i]).length()) / radii[i].min(radii[j]))
            .fold(0.0f32, f32::max);
        let (min, max) = pos.iter().zip(&radii).fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), (p, r)| {
            (lo.min(*p - Vec2::splat(*r)), hi.max(*p + Vec2::splat(*r)))
        });
        let shift = -(min + max) / 2.0;
        best = (pos.iter().map(|p| area.center() + *p + shift).collect(), radii);
        if worst < 0.05 {
            break;
        }
        scale *= 0.9;
    }
    best
}

impl App {
    pub(super) fn draw_background(&mut self, ui: &Ui, screen: Rect) {
        let wanted = if self.theme().settings.background.game_art {
            self.selected_game().and_then(|g| g.asset(AssetKind::Background).map(Path::to_path_buf))
        } else {
            None
        };
        if wanted != self.bg_current {
            self.bg_previous = std::mem::replace(&mut self.bg_current, wanted);
            self.bg_changed = Instant::now();
        }
        let t = (self.bg_changed.elapsed().as_secs_f32() / BG_FADE.as_secs_f32()).min(1.0);

        paint_theme_background(ui, self.theme(), screen);
        let prev_alpha = if self.bg_previous.is_some() { 1.0 - t } else { 0.0 };
        let cur_alpha = if self.bg_current.is_some() { t } else { 0.0 };
        if let Some(prev) = &self.bg_previous
            && prev_alpha > 0.0
        {
            paint_image_cover(ui, prev, screen, prev_alpha, 0);
        }
        if let Some(cur) = &self.bg_current {
            paint_image_cover(ui, cur, screen, cur_alpha, 0);
        }
        let art = 1.0 - (1.0 - prev_alpha) * (1.0 - cur_alpha);
        if art > 0.0 {
            let dim = |a: f32| Color32::from_black_alpha((a * art) as u8);
            gradient(ui.painter(), screen, dim(90.0), dim(200.0), GradientDirection::Vertical);
        }
    }

    pub(super) fn bar_rect(&self, slot: BarSlot, screen: Rect, s: f32) -> Rect {
        let bar = self.theme().settings.bar(slot);
        let h = if bar.enabled { bar.height(slot) } else { slot.default_height() };
        let h = self.bar_minimum(slot).map_or(h, |(min, _)| h.max(min)) * s;
        match slot {
            BarSlot::Top => Rect::from_min_max(screen.min, pos2(screen.right(), screen.top() + h)),
            BarSlot::Bottom => Rect::from_min_max(pos2(screen.left(), screen.bottom() - h), screen.max),
        }
    }

    pub(super) fn bar_minimum(&self, slot: BarSlot) -> Option<(f32, String)> {
        self.addons.min_height(&self.settings, slot).map(|(h, name)| (h, name.to_owned()))
    }

    pub(super) fn draw_bars(&self, ui: &Ui, screen: Rect, s: f32) {
        for slot in BarSlot::ALL {
            paint_bar(ui, self.theme(), slot, self.bar_rect(slot, screen, s));
        }
    }

    pub(super) fn draw_widgets(&self, painter: &Painter, screen: Rect, s: f32) {
        const GAP: f32 = 36.0;
        for slot in BarSlot::ALL {
            let widgets = self.addons.widgets(&self.settings, Placement::Bar(slot));
            if widgets.is_empty() {
                continue;
            }
            let bar = self.bar_rect(slot, screen, s);
            let widths: Vec<f32> = widgets.iter().map(|(addon, status)| widget_columns(status) * WIDGET_COLUMN * AddonHost::scale(&self.settings, &addon.id)).collect();
            let total = widths.iter().sum::<f32>() + GAP * (widgets.len() - 1) as f32;
            let mut x = bar.center().x - total * s / 2.0;
            for ((addon, status), width) in widgets.iter().zip(widths) {
                let area = Rect::from_min_size(pos2(x, bar.top()), vec2(width * s, bar.height()));
                paint_widget(painter, addon, status, area, s * AddonHost::scale(&self.settings, &addon.id));
                x += (width + GAP) * s;
            }
        }
        for (addon, status) in self.addons.widgets(&self.settings, Placement::Screen) {
            let area = self.screen_widget_rect(addon, &status, screen, s);
            paint_widget(painter, addon, &status, area, s * AddonHost::scale(&self.settings, &addon.id));
        }
    }

    pub(super) fn screen_widget_rect(&self, addon: &Manifest, status: &addons::Status, screen: Rect, s: f32) -> Rect {
        let entry = self.settings.addons.get(&addon.id).cloned().unwrap_or_default();
        let scale = entry.scale();
        let size = vec2(widget_columns(status) * WIDGET_COLUMN, addon.min_height().unwrap_or(WIDGET_HEIGHT)) * scale * s;
        let center = screen.min + vec2(entry.x.clamp(0.0, 1.0) * screen.width(), entry.y.clamp(0.0, 1.0) * screen.height());
        let half = (size / 2.0).min(screen.size() / 2.0);
        let center = pos2(center.x.clamp(screen.left() + half.x, screen.right() - half.x), center.y.clamp(screen.top() + half.y, screen.bottom() - half.y));
        Rect::from_center_size(center, size)
    }

    pub(super) fn draw_top_bar(&self, painter: &Painter, screen: Rect, s: f32) {
        let y = self.bar_rect(BarSlot::Top, screen, s).center().y;
        let count = self.library.games.len();
        let label = tr_n!(count, "Library · {n} game", "Library · {n} games");
        shadow_text(painter, pos2(screen.left() + 110.0 * s, y), Align2::LEFT_CENTER, &label, 24.0 * s, Color32::from_gray(210));
        let clock = chrono::Local::now().format("%H:%M").to_string();
        shadow_text(painter, pos2(screen.right() - 60.0 * s, y), Align2::RIGHT_CENTER, &clock, 30.0 * s, Color32::WHITE);
        if self.global_mouse.connected() {
            let right = pos2(screen.right() - 160.0 * s, y);
            let text = tr!("Global Virtual Mouse");
            let width = painter.layout_no_wrap(text.to_owned(), FontId::proportional(18.0 * s), PAD_A).size().x;
            painter.circle_filled(right - vec2(width + 14.0 * s, 0.0), 6.0 * s, PAD_A);
            shadow_text(painter, right, Align2::RIGHT_CENTER, text, 18.0 * s, PAD_A);
        }
    }

    pub(super) fn home_layout(&self) -> Layout {
        let theme = &self.theme().settings;
        match theme.view {
            ViewKind::Row => Layout::Row { rows: theme.tiles.rows.max(1) as usize },
            ViewKind::Grid => Layout::Grid { columns: theme.grid.columns.max(1) as usize, rows: theme.grid.rows.max(1) as usize },
        }
    }

    pub(super) fn draw_row(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let tiles = &self.theme().settings.tiles;
        let rows = tiles.rows.max(1) as usize;
        let mut small = self.crt_height().unwrap_or(tiles.size) * s;
        let mut big = if tiles.enlarge_selected { small * tiles.selected_scale } else { small };
        let gap = tiles.spacing * s;
        let left = screen.left() + 110.0 * s;
        let details = 200.0 * s;
        let room = screen.height() - 260.0 * s - details - (rows - 1) as f32 * gap;
        let fit = (room / ((rows - 1) as f32 * small + big)).clamp(0.3, 1.0);
        small *= fit;
        big *= fit;
        let tiles_h = (rows - 1) as f32 * (small + gap) + big;
        let position = if rows == 1 { tiles.bar_position } else { BarPosition::Top };
        let top = match position {
            BarPosition::Top => screen.top() + 150.0 * s,
            BarPosition::Middle => screen.center().y - (tiles_h + details) / 2.0,
            BarPosition::Bottom => screen.bottom() - 110.0 * s - tiles_h - details,
        }
        .max(screen.top() + 90.0 * s);
        let radius = (6.0 * s) as u8;
        let aspect = self.theme().settings.tile_aspect();

        let ctx = ui.ctx();
        let scroll_x = ctx.animate_value_with_time(Id::new("row-scroll"), (self.selected / rows) as f32, 0.18);
        let scroll_y = ctx.animate_value_with_time(Id::new("row-scroll-y"), (self.selected % rows) as f32, 0.18);
        let interactive = matches!(self.mode, Mode::Home | Mode::SwapTile { .. });
        let mut clicked = None;
        let mut shown = Vec::new();

        for i in 0..self.tile_count() {
            let px = (i / rows) as f32 - scroll_x;
            let py = (i % rows) as f32 - scroll_y;
            let focus = (1.0 - px.abs()).max(0.0) * (1.0 - py.abs()).max(0.0);
            let size = small + (big - small) * focus;
            let x = left + px * (small * aspect + gap) + (big - small) * aspect * px.clamp(0.0, 1.0);
            let y = top + py * (small + gap) + (big - small) * py.clamp(0.0, 1.0);
            let rect = Rect::from_min_size(pos2(x, y), vec2(size * aspect, size));
            if rect.right() < screen.left() || rect.left() > screen.right() {
                continue;
            }
            shown.push(i);
            self.paint_tile(ui, i, rect, radius, s, focus);
            if interactive && ui.interact(rect, Id::new(("tile", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }

        self.crt_visible = shown;
        if self.draw_details(ui, pos2(left, top + tiles_h + 34.0 * s), s, true) {
            clicked = Some(self.selected);
        }
        if let Some(i) = clicked {
            self.click_tile(i);
        }
    }

    fn crt_height(&self) -> Option<f32> {
        let theme = &self.theme().settings;
        (theme.tile_style == TileStyle::Crt).then(|| theme.crt.size()[1])
    }

    pub(super) fn draw_grid(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let grid = &self.theme().settings.grid;
        let (columns, rows) = (grid.columns.max(1) as usize, grid.rows.max(1) as usize);
        let per_page = columns * rows;
        let count = self.tile_count();
        let pages = count.div_ceil(per_page);
        let page = self.selected / per_page;
        let gap = grid.spacing * s;
        let enlarge = grid.enlarge_selected;
        let (style, size) = (self.theme().settings.tile_style, self.theme().settings.boxes.size_mm());
        let radius = (6.0 * s) as u8;

        let area = Rect::from_min_max(
            pos2(screen.left() + 110.0 * s, screen.top() + 100.0 * s),
            pos2(screen.right() - 110.0 * s, screen.bottom() - 250.0 * s),
        );
        let aspect = self.theme().settings.tile_aspect();
        let cell_h = ((area.width() - (columns - 1) as f32 * gap) / columns as f32 / aspect)
            .min((area.height() - (rows - 1) as f32 * gap) / rows as f32)
            .min(self.crt_height().map_or(f32::INFINITY, |h| h * s))
            .max(8.0 * s);
        let cell = vec2(cell_h * aspect, cell_h);
        let grid_size = vec2(columns as f32 * (cell.x + gap) - gap, rows as f32 * (cell.y + gap) - gap);
        let x = match grid.align {
            GridAlign::Left => area.left(),
            GridAlign::Center => area.center().x - grid_size.x / 2.0,
        };
        let origin = pos2(x, area.center().y - grid_size.y / 2.0);

        let ctx = ui.ctx().clone();
        let slide = ctx.animate_value_with_time(Id::new("grid-page"), page as f32, 0.25);
        let interactive = matches!(self.mode, Mode::Home | Mode::SwapTile { .. });
        let mut clicked = None;
        let cell_pos = |i: usize, offset: f32| {
            let (col, row) = (i % per_page % columns, i % per_page / columns);
            origin + vec2(offset * screen.width() + col as f32 * (cell.x + gap), row as f32 * (cell.y + gap))
        };

        let last_offset = (pages - 1) as f32 - slide;
        if last_offset.abs() < 1.0 {
            for i in count..pages * per_page {
                let rect = Rect::from_min_size(cell_pos(i, last_offset), cell);
                let (fill, stroke) = (Color32::from_white_alpha(10), Stroke::new(1.0, Color32::from_white_alpha(22)));
                let outline = match style {
                    TileStyle::Flat | TileStyle::Cover => {
                        ui.painter().rect_filled(rect, radius, fill);
                        ui.painter().rect_stroke(rect, radius, stroke, StrokeKind::Inside);
                        continue;
                    }
                    TileStyle::Model => box3d::outline(rect, size, box3d::REST_YAW),
                    TileStyle::Case => case3d::outline(rect, size, case3d::Pose::CLOSED),
                    TileStyle::Disc => disc3d::outline(rect, self.disc_pose(usize::MAX)),
                    TileStyle::Crt => crt::outline(rect),
                    TileStyle::Object => {
                        let (floor, radius) = object_floor(rect);
                        ui.painter().add(Shape::ellipse_filled(floor, radius, fill));
                        ui.painter().add(Shape::ellipse_stroke(floor, radius, stroke));
                        continue;
                    }
                };
                ui.painter().add(Shape::convex_polygon(outline, fill, stroke));
            }
        }

        let order = (0..count).filter(|&i| i != self.selected).chain([self.selected.min(count - 1)]);
        let mut shown = Vec::new();
        for i in order {
            let offset = (i / per_page) as f32 - slide;
            if offset.abs() >= 1.0 {
                continue;
            }
            shown.push(i);
            let focus = ctx.animate_bool_with_time(Id::new(("grid-focus", i)), i == self.selected, 0.15);
            let grow = if enlarge { cell_h * 0.08 * focus } else { 0.0 };
            let rect = Rect::from_min_size(cell_pos(i, offset), cell).expand(grow);
            self.paint_tile(ui, i, rect, radius, s, focus);
            if interactive && i / per_page == page && ui.interact(rect, Id::new(("tile", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }

        self.crt_visible = shown;
        if let Some(page) = self.draw_page_arrows(ui, screen, s, origin.y + grid_size.y / 2.0, (page, pages), interactive) {
            self.selected = (page * per_page + self.selected % per_page).min(count - 1);
            self.swap_sound();
        }

        self.draw_details(ui, pos2(screen.left() + 110.0 * s, screen.bottom() - 200.0 * s), s, false);
        if let Some(i) = clicked {
            self.click_tile(i);
        }
    }

    fn draw_page_arrows(&self, ui: &Ui, screen: Rect, s: f32, arrow_y: f32, (page, pages): (usize, usize), interactive: bool) -> Option<usize> {
        if pages <= 1 {
            return None;
        }
        let painter = ui.painter();
        let mut clicked = None;
        for (target, show, x, glyph) in [
            (page.wrapping_sub(1), page > 0, screen.left() + 55.0 * s, "‹"),
            (page + 1, page + 1 < pages, screen.right() - 55.0 * s, "›"),
        ] {
            if !show {
                continue;
            }
            let hit = Rect::from_center_size(pos2(x, arrow_y), vec2(80.0 * s, 160.0 * s));
            shadow_text(painter, hit.center(), Align2::CENTER_CENTER, glyph, 64.0 * s, Color32::WHITE);
            if interactive && ui.interact(hit, Id::new(("page-arrow", target > page)), Sense::click()).clicked() {
                clicked = Some(target);
            }
        }
        let counter = tr!("Page {page} / {pages}", page = page + 1, pages = pages);
        // After the system options hint, which takes the bottom left corner.
        let left = self.system_hint_right(painter, screen, s) + 30.0 * s;
        let pos = pos2(left, self.bar_rect(BarSlot::Bottom, screen, s).center().y);
        shadow_text(painter, pos, Align2::LEFT_CENTER, &counter, 22.0 * s, Color32::from_gray(210));
        clicked
    }

    pub(super) fn is_bubbles(&self) -> bool {
        self.theme().settings.kind == ThemeKind::Bubbles
    }

    pub(super) fn bubble_occupant(&self, slot: usize) -> Option<usize> {
        self.bubble_places().iter().position(|&p| p == slot)
    }

    pub(super) fn empty_bubble(&self) -> Option<usize> {
        let slot = self.bubble_empty.filter(|_| self.is_bubbles() && matches!(self.mode, Mode::Home))?;
        let total = self.bubble_layout.as_ref().map_or(0, |l| l.centers.len());
        (slot < total && self.bubble_occupant(slot).is_none()).then_some(slot)
    }

    pub(super) fn navigate_bubbles(&mut self, action: Action) {
        let from = self.empty_bubble().unwrap_or_else(|| self.bubble_places().get(self.selected).copied().unwrap_or(self.selected));
        let to = match self.bubble_layout.as_ref() {
            Some(l) if l.centers.len() >= self.tile_count() => navigate_bubble_pages(&l.centers, l.per_page, l.area, from, action),
            _ => return,
        };
        if to == from {
            return;
        }
        self.focus_bubble(to);
        self.bubble_sound(to);
    }

    fn focus_bubble(&mut self, slot: usize) {
        if let Mode::SwapBubble { target, .. } = &mut self.mode {
            *target = slot;
            return;
        }
        match self.bubble_occupant(slot) {
            Some(i) => {
                self.selected = i;
                self.bubble_empty = None;
            }
            None => self.bubble_empty = Some(slot),
        }
    }

    pub(super) fn bubble_sound(&mut self, slot: usize) {
        if !self.theme().settings.bubbles.sound || self.custom_swap_sound(true) {
            return;
        }
        let pitch = {
            let Some(layout) = self.bubble_layout.as_ref() else { return };
            let Some(&r) = layout.radii.get(slot) else { return };
            let mean = layout.radii.iter().sum::<f32>() / layout.radii.len() as f32;
            (620.0 * (mean / r.max(1.0)).powf(0.7)).clamp(300.0, 1400.0)
        };
        self.sounds.bubble(pitch);
    }

    pub(super) fn custom_swap_sound(&mut self, bubbles: bool) -> bool {
        let theme = self.theme();
        let custom = if bubbles { theme.settings.bubbles.custom_sound } else { theme.settings.swap.custom_sound };
        match theme.sound_file(bubbles).filter(|_| custom) {
            Some(path) => self.sounds.custom(&path),
            None => false,
        }
    }

    pub(super) fn swap_sound(&mut self) {
        if self.theme().settings.swap.sound && !self.custom_swap_sound(false) {
            self.sounds.swap();
        }
    }

    pub(super) fn play_swap_sound(&mut self, bubbles: bool) {
        if !self.custom_swap_sound(bubbles) && !bubbles {
            self.sounds.swap();
        }
    }

    pub(super) fn update_videos(&mut self, ctx: &Context) {
        const MAX_IDLE: usize = 12;
        let theme = &self.theme().settings;
        let showing = matches!(self.mode, Mode::Home) && ctx.input(|i| i.focused);
        let mut wanted = Vec::new();
        if showing && theme.kind == ThemeKind::Bubbles && theme.bubbles.play_video && self.empty_bubble().is_none() {
            wanted.extend(self.selected_game().and_then(|g| g.asset(AssetKind::Video)).map(|p| (p.to_path_buf(), video::Format::BUBBLE)));
        } else if showing && theme.kind == ThemeKind::Standard && theme.tile_style == TileStyle::Crt {
            let mut visible = self.crt_visible.clone();
            visible.sort_by_key(|&i| i.abs_diff(self.selected));
            let mut idle = 0;
            for i in visible {
                let Some(game) = self.library.games.get(i) else { continue };
                match self.crt_video(game, i == self.selected) {
                    Some((_, video::Format::CRT_IDLE)) if idle >= MAX_IDLE => {}
                    Some((path, format)) => {
                        idle += usize::from(format == video::Format::CRT_IDLE);
                        wanted.push((path.to_path_buf(), format));
                    }
                    None => {}
                }
            }
        }
        if let Some(e) = self.videos.get_mut().sync(ctx, &wanted, VIDEO_DELAY) {
            self.notify(tr!("Videos need ffmpeg installed: {error}", error = format!("{e:#}")));
        }
    }

    pub(super) fn draw_bubbles(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let count = self.tile_count();
        let bubbles = self.theme().settings.bubbles.clone();
        let top = self.bar_rect(BarSlot::Top, screen, s).bottom() + 16.0 * s;
        let mut area = Rect::from_min_max(pos2(screen.left() + 60.0 * s, top), pos2(screen.right() - 60.0 * s, screen.bottom() - 220.0 * s));
        let per_page = bubble_capacity(bubbles.size, area, s);
        // Games keep their saved bubble, so the pages reach far enough to hold the last one.
        let used = match bubbles.fill_screen {
            true => self.library.games.iter().filter_map(|g| g.bubble_slot).filter(|&j| j < count * per_page).map(|j| j + 1).max().unwrap_or(0),
            false => 0,
        };
        let pages = count.max(used).div_ceil(per_page).max(1);
        if pages > 1 {
            area = area.shrink2(vec2(50.0 * s, 0.0));
        }
        // Sizes belong to the bubbles, not to the games in them, so the layout only changes when the page count does.
        let total = if bubbles.fill_screen { pages * per_page } else { count };
        let diameters: Vec<f32> = (0..total).map(|j| bubbles.diameter(&format!("slot-{j}")) * s).collect();
        if self.bubble_layout.as_ref().is_none_or(|c| c.diameters != diameters || c.area != area || c.per_page != per_page) {
            let (mut centers, mut radii) = (Vec::new(), Vec::new());
            for page in diameters.chunks(per_page) {
                let (c, r) = pack_bubbles(page, area);
                centers.extend(c);
                radii.extend(r);
            }
            self.bubble_layout = Some(BubbleLayout { diameters, area, per_page, centers, radii });
        }
        let (centers, radii) = self.bubble_layout.as_ref().map(|l| (l.centers.clone(), l.radii.clone())).unwrap_or_default();
        if centers.len() < count {
            return;
        }
        self.keep_bubble_places();
        let places = self.bubble_places();
        let slot = |i: usize| places.get(i).copied().unwrap_or(i);
        let mut occupant: Vec<Option<usize>> = vec![None; centers.len()];
        for i in 0..count {
            occupant[slot(i)] = Some(i);
        }

        let ctx = ui.ctx().clone();
        let video = match self.selected_game().and_then(|g| g.asset(AssetKind::Video)) {
            Some(path) if bubbles.play_video => self.videos.borrow_mut().frame(&ctx, path),
            _ => None,
        };
        let t = ctx.input(|i| i.time) as f32;
        let interactive = matches!(self.mode, Mode::Home | Mode::SwapBubble { .. });
        let (swapping, target) = match self.mode {
            Mode::SwapBubble { from, target } => (Some(from), Some(target)),
            _ => (None, None),
        };
        let selected = self.selected.min(count - 1);
        let empty = self.empty_bubble();
        let focused = target.or(empty).unwrap_or(slot(selected));
        let moving = swapping.map(slot);
        let page = focused / per_page;
        let slide = ctx.animate_value_with_time(Id::new("bubble-page"), page as f32, 0.25);
        let mut clicked = None;
        let order = (0..centers.len())
            .filter(|&j| occupant[j].is_none() && j != focused && Some(j) != moving)
            .chain((0..centers.len()).filter(|&j| occupant[j].is_some() && j != focused && Some(j) != moving))
            .chain(moving.filter(|&j| j != focused))
            .chain([focused]);
        for j in order {
            let offset = (j / per_page) as f32 - slide;
            if offset.abs() >= 1.0 {
                continue;
            }
            let tile = occupant[j];
            let focus = ctx.animate_bool_with_time(Id::new(("bubble-focus", j)), j == focused, 0.18);
            let phase = tile.unwrap_or(j) as f32 * 1.7;
            let wobble = vec2((t * 0.5 + phase).cos() * 3.0 * s, (t * 0.7 + phase).sin() * 4.0 * s);
            let center = centers[j] + wobble + vec2(offset * screen.width(), 0.0);
            let r = radii[j] * (1.0 + 0.18 * focus);
            let game = tile.and_then(|i| self.library.games.get(i));
            let cover = game.and_then(|g| g.asset(AssetKind::Cover)).and_then(|path| {
                match egui::Image::new(file_uri(path)).load_for_size(&ctx, Vec2::splat(r * 2.0)) {
                    Ok(egui::load::TexturePoll::Ready { texture }) => Some((texture.id, texture.size)),
                    _ => None,
                }
            });
            let look = BubbleLook {
                empty: tile.is_none(),
                texture: video.filter(|_| tile == Some(selected) && game.is_some_and(|g| g.asset(AssetKind::Video).is_some())).or(cover),
                fill: game.map_or(Color32::from_white_alpha(40), |g| placeholder_color(&g.name)),
                label: match (game, tile) {
                    (Some(g), _) => initials(&g.name),
                    (None, Some(i)) => self.special(i).map_or("", Special::glyph).to_owned(),
                    (None, None) => String::new(),
                },
                icon: tile.filter(|_| game.is_none()).and_then(|i| self.special(i)).and_then(|s| self.special_icon(s)),
                swapping: Some(j) == moving,
                running: tile.is_some_and(|i| self.is_active(i)),
            };
            paint_bubble(ui.painter(), center, r, &look, t, focus, s);
            if interactive && j / per_page == page {
                let resp = ui.interact(Rect::from_center_size(center, Vec2::splat(r * 2.0)), Id::new(("bubble", j)), Sense::click());
                if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| p.distance(center) <= r) {
                    clicked = Some(j);
                }
            }
        }

        let arrows = self.draw_page_arrows(ui, screen, s, area.center().y, (page, pages), interactive);
        if let Some(to) = arrows.and_then(|p| bubble_page_entry(&centers, per_page, area, focused, p)) {
            self.focus_bubble(to);
            self.bubble_sound(to);
        }

        let details = pos2(screen.left() + 110.0 * s, screen.bottom() - 200.0 * s);
        if let Some(name) = swapping.and_then(|i| self.library.games.get(i)).map(|g| g.name.clone()) {
            let text = tr!("Pick a bubble to swap places with {name}", name = name);
            shadow_text(ui.painter(), details - vec2(0.0, 44.0 * s), Align2::LEFT_TOP, &text, 24.0 * s, SWAP_GLOW);
        }
        if empty.is_none() {
            self.draw_details(ui, details, s, false);
        }
        match (clicked, swapping) {
            (Some(j), Some(from)) => self.move_bubble(from, j),
            (Some(j), None) => {
                if j != focused {
                    self.bubble_sound(j);
                }
                self.bubble_empty = None;
                match occupant[j] {
                    Some(i) if empty.is_some() => self.selected = i,
                    Some(i) => self.click_tile(i),
                    None => self.bubble_empty = Some(j),
                }
            }
            _ => {}
        }
    }

    /// The bubble each tile sits in. Games keep their saved bubble, the special tiles take the last
    /// bubbles of the first page, and a game without one takes the first empty bubble.
    pub(super) fn bubble_places(&self) -> Vec<usize> {
        let games = &self.library.games;
        let (n, count) = (games.len(), self.tile_count());
        let (total, per_page) = self.bubble_layout.as_ref().map_or((count, count), |l| (l.centers.len(), l.per_page));
        let total = total.max(count);
        let index: std::collections::HashMap<&str, usize> = games.iter().enumerate().map(|(i, g)| (g.id.as_str(), i)).collect();
        let legacy = |key: &str| match key.strip_prefix("empty-") {
            Some(k) => k.parse::<usize>().ok().map(|k| count + k),
            None => index.get(key).copied(),
        };
        let mut place = vec![usize::MAX; count];
        let mut taken = vec![false; total];
        for (i, game) in games.iter().enumerate() {
            let saved = game.bubble_slot.or_else(|| game.bubble_place.as_deref().and_then(legacy));
            if let Some(slot) = saved.filter(|&j| j < total && !taken[j]) {
                place[i] = slot;
                taken[slot] = true;
            }
        }
        let first = per_page.min(total).saturating_sub(count - n);
        for (i, slot) in (n..count).zip(first..) {
            if !taken[slot] {
                place[i] = slot;
                taken[slot] = true;
            }
        }
        let mut free = (0..total).filter(|&j| !taken[j]);
        for p in place.iter_mut().filter(|p| **p == usize::MAX) {
            *p = free.next().unwrap_or(0);
        }
        place
    }

    /// Saves the bubble each game sits in, so a new game fills an empty bubble instead of moving the others.
    fn keep_bubble_places(&mut self) {
        let places = self.bubble_places();
        let mut changed = false;
        for (game, &slot) in self.library.games.iter_mut().zip(&places) {
            if game.bubble_slot != Some(slot) || game.bubble_place.is_some() {
                game.bubble_slot = Some(slot);
                game.bubble_place = None;
                changed = true;
            }
        }
        if changed {
            self.save();
        }
    }

    pub(super) fn move_bubble(&mut self, from: usize, target: usize) {
        let places = self.bubble_places();
        let other = places.iter().position(|&p| p == target);
        if from >= self.library.games.len() || other.is_some_and(|o| o >= self.library.games.len()) {
            self.notify(tr!("Only games can swap places"));
            return;
        }
        self.mode = Mode::Home;
        let Some(&own) = places.get(from) else { return };
        if own == target {
            return;
        }
        self.library.games[from].bubble_slot = Some(target);
        if let Some(other) = other {
            self.library.games[other].bubble_slot = Some(own);
        }
        self.save();
    }

    pub(super) fn draw_details(&self, ui: &Ui, pos: egui::Pos2, s: f32, start_button: bool) -> bool {
        let (x, y) = (pos.x, pos.y);
        let painter = ui.painter();
        let mut clicked = false;
        match self.selected_game() {
            Some(game) => {
                shadow_text_font(painter, pos2(x, y), Align2::LEFT_TOP, &game.name, self.game_title_font(game, 46.0 * s), Color32::WHITE);
                if let Mode::SwapTile { from } = self.mode {
                    let name = self.library.games.get(from).map_or("", |g| g.name.as_str());
                    let text = tr!("Pick a game to swap places with {name}", name = name);
                    shadow_text(painter, pos2(x, y + 62.0 * s), Align2::LEFT_TOP, &text, 22.0 * s, SWAP_GLOW);
                    return false;
                }
                let sub = self.game_status(game);
                shadow_text(painter, pos2(x, y + 62.0 * s), Align2::LEFT_TOP, &sub, 22.0 * s, Color32::from_gray(190));
                if start_button {
                    let btn = Rect::from_min_size(pos2(x, y + 110.0 * s), vec2(220.0 * s, 56.0 * s));
                    painter.rect_filled(btn, (4.0 * s) as u8, Color32::WHITE);
                    let label = if self.is_active(self.selected) { tr!("Switch") } else { tr!("Start") };
                    painter.text(btn.center(), Align2::CENTER_CENTER, label, FontId::proportional(26.0 * s), Color32::BLACK);
                    clicked = matches!(self.mode, Mode::Home) && ui.interact(btn, Id::new("start-btn"), Sense::click()).clicked();
                }
            }
            None => {
                let special = self.special(self.selected).unwrap_or(Special::Settings);
                let sub = special_description(special);
                let title = special.name();
                shadow_text(painter, pos2(x, y), Align2::LEFT_TOP, title, 46.0 * s, Color32::WHITE);
                shadow_text(painter, pos2(x, y + 62.0 * s), Align2::LEFT_TOP, sub, 22.0 * s, Color32::from_gray(190));
            }
        }
        clicked
    }

    /// The line under the selected game's name: when it was played, and what is going on with it.
    pub(super) fn game_status(&self, game: &Game) -> String {
        let sub = match game.last_played.and_then(|t| chrono::DateTime::from_timestamp(t as i64, 0)) {
            Some(t) => tr!("Last played {date}", date = t.with_timezone(&chrono::Local).format(tr!("%d %b %Y, %H:%M"))),
            None => tr!("Never played").to_owned(),
        };
        let sub = match self.artwork.status(&game.id) {
            Some(metadata::Status::Done(at, _)) if at.elapsed() >= metadata::DONE_BADGE => sub,
            Some(status) => format!("{sub}  ·  {}", status.describe()),
            None => sub,
        };
        let index = self.library.position(&game.id);
        let sub = if index.is_some_and(|i| self.is_active(i)) {
            tr!("Running").to_owned()
        } else if let Some(update) = self.game_update(game) {
            tr!("{status}  ·  Update to {version} in Options", status = sub, version = update.latest.version)
        } else {
            sub
        };
        match (game.fixed_slot, index) {
            (Some(_), Some(i)) if self.is_standard() && !self.settings.order.static_order => {
                tr!("{status}  ·  Static position: slot {slot}", status = sub, slot = i + 1)
            }
            _ => sub,
        }
    }

    pub(super) fn click_tile(&mut self, i: usize) {
        if let Mode::SwapTile { from } = self.mode {
            return self.finish_swap(from, i);
        }
        if i == self.selected {
            self.activate_selected();
        } else {
            self.selected = i;
            if !self.is_bubbles() {
                self.swap_sound();
            }
        }
    }
}

impl App {
    pub(super) fn handle_home(&mut self, action: Action) {
        let Mode::Home = &mut self.mode else { return };
        match action {
            Action::Left | Action::Right | Action::Up | Action::Down => {
                if self.is_bubbles() {
                    self.navigate_bubbles(action);
                } else if self.custom_running() {
                    self.navigate_custom(action);
                } else {
                    let before = self.selected;
                    self.selected = navigate(self.home_layout(), self.selected, self.tile_count(), action);
                    if self.selected != before {
                        self.swap_sound();
                    }
                }
            }
            Action::Confirm | Action::Options if self.empty_bubble().is_some() => {}
            Action::Confirm => self.activate_selected(),
            Action::Options if self.selected_game().is_some() => {
                self.mode = Mode::Options { cursor: 0 }
            }
            Action::Add | Action::AddGame => self.mode = Mode::AddMenu { cursor: 0 },
            Action::OpenStore => self.open_store(),
            _ => {}
        }
    }

    pub(super) fn handle_swap_tile(&mut self, action: Action) {
        let Mode::SwapTile { from } = self.mode else { return };
        match action {
            Action::Left | Action::Right | Action::Up | Action::Down => {
                let before = self.selected;
                self.selected = navigate(self.home_layout(), self.selected, self.library.games.len(), action);
                if self.selected != before {
                    self.swap_sound();
                }
            }
            Action::Confirm => self.finish_swap(from, self.selected),
            Action::Back => {
                self.selected = from;
                self.mode = Mode::Options { cursor: self.menu_index(MenuItem::SwapTile) };
            }
            _ => {}
        }
    }

    /// Swaps the games at `from` and `to` and leaves the moved game selected in its new slot.
    pub(super) fn finish_swap(&mut self, from: usize, to: usize) {
        let games = self.library.games.len();
        if to >= games {
            self.notify(tr!("Only games can swap places"));
            return;
        }
        self.mode = Mode::Home;
        if from < games && from != to {
            self.library.swap(from, to);
            self.save();
        }
        self.selected = to;
    }

    pub(super) fn handle_swap_bubble(&mut self, action: Action) {
        let Mode::SwapBubble { from, target } = &mut self.mode else { return };
        match action {
            Action::Left | Action::Right | Action::Up | Action::Down => {
                let from = *target;
                let to = match self.bubble_layout.as_ref() {
                    Some(l) => navigate_bubble_pages(&l.centers, l.per_page, l.area, from, action),
                    None => from,
                };
                *target = to;
                if to != from {
                    self.bubble_sound(to);
                }
            }
            Action::Confirm => {
                let (from, target) = (*from, *target);
                self.move_bubble(from, target);
            }
            Action::Back => self.mode = Mode::Options { cursor: self.menu_index(MenuItem::SwapBubble) },
            _ => {}
        }
    }
}

pub(super) const WIDGET_COLUMN: f32 = 130.0;
const WIDGET_HEIGHT: f32 = 110.0;

fn widget_columns(status: &addons::Status) -> f32 {
    match status {
        addons::Status::Showing(frame) => frame.items.len().max(1) as f32,
        _ => 2.0,
    }
}

fn paint_widget(painter: &Painter, addon: &Manifest, status: &addons::Status, area: Rect, s: f32) {
    match status {
        addons::Status::Showing(frame) => {
            for (i, item) in frame.items.iter().enumerate() {
                let cell = Rect::from_min_size(pos2(area.left() + i as f32 * WIDGET_COLUMN * s, area.top()), vec2(WIDGET_COLUMN * s, area.height()));
                paint_widget_item(painter, item, cell, s);
            }
        }
        addons::Status::Starting => {
            shadow_text(painter, area.center(), Align2::CENTER_CENTER, &addon.name, 18.0 * s, Color32::from_gray(150));
        }
        addons::Status::Failed(error) => {
            let text = tr!("{name}: {error}", name = addon.name, error = error);
            let painter = painter.with_clip_rect(area.expand2(vec2(WIDGET_COLUMN * s, 0.0)));
            shadow_text(&painter, area.center(), Align2::CENTER_CENTER, &text, 16.0 * s, PAD_B);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Layout, navigate, navigate_bubble_pages, navigate_free, pack_bubbles};
    use egui::{Rect, pos2};

    #[test]
    fn bubbles_fit_on_screen_without_overlapping() {
        let area = Rect::from_min_max(pos2(60.0, 100.0), pos2(1860.0, 860.0));
        for n in [1, 3, 20, 80] {
            let diameters: Vec<f32> = (0..n).map(|i| 130.0 + (i * 7 % 41) as f32).collect();
            let (centers, radii) = pack_bubbles(&diameters, area);
            assert_eq!(centers.len(), n);
            for i in 0..n {
                let r = radii[i];
                assert!(area.expand(1.0).contains_rect(Rect::from_center_size(centers[i], egui::Vec2::splat(r * 2.0))), "{n} bubbles: {i} sticks out");
                for j in i + 1..n {
                    let overlap = radii[i] + radii[j] - centers[i].distance(centers[j]);
                    assert!(overlap < 0.05 * radii[i].min(radii[j]), "{n} bubbles: {i} and {j} overlap by {overlap}");
                }
            }
        }
    }

    #[test]
    fn bubbles_move_to_the_nearest_one_in_that_direction() {
        let centers = [pos2(500.0, 500.0), pos2(700.0, 520.0), pos2(300.0, 480.0), pos2(510.0, 300.0), pos2(900.0, 500.0)];
        assert_eq!(navigate_free(&centers, 0, Action::Right), 1);
        assert_eq!(navigate_free(&centers, 0, Action::Left), 2);
        assert_eq!(navigate_free(&centers, 0, Action::Up), 3);
        assert_eq!(navigate_free(&centers, 0, Action::Down), 0);
        assert_eq!(navigate_free(&centers, 4, Action::Right), 4);
    }

    #[test]
    fn bubbles_cross_to_the_edge_of_the_next_page() {
        let area = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 600.0));
        let page = [pos2(100.0, 200.0), pos2(500.0, 400.0), pos2(900.0, 200.0)];
        let centers: Vec<_> = page.iter().chain(&page).copied().collect();
        let nav = |from, action| navigate_bubble_pages(&centers, 3, area, from, action);
        assert_eq!(nav(0, Action::Right), 1);
        assert_eq!(nav(2, Action::Right), 3);
        assert_eq!(nav(3, Action::Left), 2);
        assert_eq!(nav(0, Action::Left), 0);
        assert_eq!(nav(5, Action::Right), 5);
    }

    #[test]
    fn single_row_moves_sideways_only() {
        let row = Layout::Row { rows: 1 };
        assert_eq!(navigate(row, 0, 5, Action::Left), 0);
        assert_eq!(navigate(row, 0, 5, Action::Right), 1);
        assert_eq!(navigate(row, 4, 5, Action::Right), 4);
        assert_eq!(navigate(row, 2, 5, Action::Up), 2);
        assert_eq!(navigate(row, 2, 5, Action::Down), 2);
    }

    #[test]
    fn several_rows_fill_columns() {
        let rows = Layout::Row { rows: 3 };
        assert_eq!(navigate(rows, 0, 7, Action::Down), 1);
        assert_eq!(navigate(rows, 2, 7, Action::Down), 2);
        assert_eq!(navigate(rows, 4, 7, Action::Left), 1);
        assert_eq!(navigate(rows, 4, 7, Action::Right), 6);
        assert_eq!(navigate(rows, 6, 7, Action::Right), 6);
        assert_eq!(navigate(rows, 6, 7, Action::Down), 6);
    }

    #[test]
    fn grid_pages() {
        let grid = Layout::Grid { columns: 2, rows: 2 };
        assert_eq!(navigate(grid, 1, 7, Action::Right), 4);
        assert_eq!(navigate(grid, 3, 7, Action::Right), 6);
        assert_eq!(navigate(grid, 6, 7, Action::Left), 3);
        assert_eq!(navigate(grid, 4, 7, Action::Left), 1);
        assert_eq!(navigate(grid, 5, 7, Action::Down), 6);
        assert_eq!(navigate(grid, 6, 7, Action::Up), 4);
        assert_eq!(navigate(grid, 6, 7, Action::Right), 6);
        assert_eq!(navigate(grid, 0, 7, Action::Left), 0);

        let grid = Layout::Grid { columns: 8, rows: 6 };
        assert_eq!(navigate(grid, 7, 60, Action::Right), 48);
        assert_eq!(navigate(grid, 15, 60, Action::Right), 56);
        assert_eq!(navigate(grid, 48, 60, Action::Left), 7);
        assert_eq!(navigate(grid, 56, 60, Action::Left), 15);
        assert_eq!(navigate(grid, 47, 60, Action::Right), 59);
    }
}
