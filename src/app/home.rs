//! The home screen: background, bars, and the row, grid and bubble views with their navigation.

use super::*;

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
    pub(super) centers: Vec<Pos2>,
    pub(super) radii: Vec<f32>,
}

/// Lays out circles of the given diameters inside `area` without overlap, returning their centers
/// and (possibly scaled-down) radii in the same order.
///
/// Circles start on a golden-angle spiral (stretched to the area's aspect ratio), then a simple
/// relaxation pushes every overlapping pair apart while a weak pull toward the middle keeps the
/// cluster compact; the last steps drop the pull so the separation settles. If some pair still
/// overlaps by more than 5% of the smaller radius, everything is shrunk by 10% and laid out again.
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
        const COLUMN: f32 = 130.0;
        const GAP: f32 = 36.0;
        for slot in BarSlot::ALL {
            let widgets = self.addons.widgets(&self.settings, slot);
            if widgets.is_empty() {
                continue;
            }
            let bar = self.bar_rect(slot, screen, s);
            let columns: Vec<f32> = widgets
                .iter()
                .map(|(_, status)| match status {
                    addons::Status::Showing(frame) => frame.items.len().max(1) as f32 * COLUMN,
                    _ => 2.0 * COLUMN,
                })
                .collect();
            let total = columns.iter().sum::<f32>() + GAP * (widgets.len() - 1) as f32;
            let mut x = bar.center().x - total * s / 2.0;
            for ((addon, status), width) in widgets.iter().zip(columns) {
                let area = Rect::from_min_size(pos2(x, bar.top()), vec2(width * s, bar.height()));
                match status {
                    addons::Status::Showing(frame) => {
                        for (i, item) in frame.items.iter().enumerate() {
                            let cell = Rect::from_min_size(pos2(area.left() + i as f32 * COLUMN * s, area.top()), vec2(COLUMN * s, area.height()));
                            paint_widget_item(painter, item, cell, s);
                        }
                    }
                    addons::Status::Starting => {
                        shadow_text(painter, area.center(), Align2::CENTER_CENTER, &addon.name, 18.0 * s, Color32::from_gray(150));
                    }
                    addons::Status::Failed(error) => {
                        let text = tr!("{name}: {error}", name = addon.name, error = error);
                        let painter = painter.with_clip_rect(area.expand2(vec2(COLUMN * s, 0.0)));
                        shadow_text(&painter, area.center(), Align2::CENTER_CENTER, &text, 16.0 * s, PAD_B);
                    }
                }
                x += (width + GAP) * s;
            }
        }
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
        let mut small = tiles.size * s;
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
        let aspect = self.theme().settings.tile_style.aspect(self.theme().settings.boxes.size_mm());

        let ctx = ui.ctx();
        let scroll_x = ctx.animate_value_with_time(Id::new("row-scroll"), (self.selected / rows) as f32, 0.18);
        let scroll_y = ctx.animate_value_with_time(Id::new("row-scroll-y"), (self.selected % rows) as f32, 0.18);
        let interactive = matches!(self.mode, Mode::Home);
        let mut clicked = None;

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
            self.paint_tile(ui, i, rect, radius, s, focus);
            if interactive && ui.interact(rect, Id::new(("tile", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }

        if self.draw_details(ui, pos2(left, top + tiles_h + 34.0 * s), s, true) {
            clicked = Some(self.selected);
        }
        if let Some(i) = clicked {
            self.click_tile(i);
        }
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
        let aspect = style.aspect(size);
        let cell_h = ((area.width() - (columns - 1) as f32 * gap) / columns as f32 / aspect)
            .min((area.height() - (rows - 1) as f32 * gap) / rows as f32)
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
        let interactive = matches!(self.mode, Mode::Home);
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
                    TileStyle::FlatBox => BoxShape::fit(rect, size).outline().to_vec(),
                    TileStyle::Model => box3d::outline(rect, size, box3d::REST_YAW),
                    TileStyle::Case => case3d::outline(rect, size, case3d::Pose::CLOSED),
                    TileStyle::Disc => disc3d::outline(rect, self.disc_pose(usize::MAX)),
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
        for i in order {
            let offset = (i / per_page) as f32 - slide;
            if offset.abs() >= 1.0 {
                continue;
            }
            let focus = ctx.animate_bool_with_time(Id::new(("grid-focus", i)), i == self.selected, 0.15);
            let grow = if enlarge { cell_h * 0.08 * focus } else { 0.0 };
            let rect = Rect::from_min_size(cell_pos(i, offset), cell).expand(grow);
            self.paint_tile(ui, i, rect, radius, s, focus);
            if interactive && i / per_page == page && ui.interact(rect, Id::new(("tile", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }

        if pages > 1 {
            let painter = ui.painter();
            let arrow_y = origin.y + grid_size.y / 2.0;
            for (dir, show, x, glyph) in [
                (-1, page > 0, screen.left() + 55.0 * s, "‹"),
                (1, page + 1 < pages, screen.right() - 55.0 * s, "›"),
            ] {
                if !show {
                    continue;
                }
                let hit = Rect::from_center_size(pos2(x, arrow_y), vec2(80.0 * s, 160.0 * s));
                shadow_text(painter, hit.center(), Align2::CENTER_CENTER, glyph, 64.0 * s, Color32::WHITE);
                if interactive && ui.interact(hit, Id::new(("grid-page-arrow", dir)), Sense::click()).clicked() {
                    let target = (page as isize + dir) as usize * per_page + self.selected % per_page;
                    self.selected = target.min(count - 1);
                    self.swap_sound();
                }
            }
            let counter = tr!("Page {page} / {pages}", page = page + 1, pages = pages);
            let pos = pos2(screen.left() + 110.0 * s, self.bar_rect(BarSlot::Bottom, screen, s).center().y);
            shadow_text(painter, pos, Align2::LEFT_CENTER, &counter, 22.0 * s, Color32::from_gray(210));
        }

        self.draw_details(ui, pos2(screen.left() + 110.0 * s, screen.bottom() - 200.0 * s), s, false);
        if let Some(i) = clicked {
            self.click_tile(i);
        }
    }

    pub(super) fn is_bubbles(&self) -> bool {
        self.theme().settings.kind == ThemeKind::Bubbles
    }

    /// The tile sitting in a bubble slot; `None` for an empty bubble.
    pub(super) fn bubble_occupant(&self, slot: usize) -> Option<usize> {
        let specials = self.library.games.len()..self.tile_count();
        self.bubble_places().iter().position(|&p| p == slot).or_else(|| specials.contains(&slot).then_some(slot))
    }

    /// The empty bubble the cursor rests on in Home, if it isn't on a tile.
    pub(super) fn empty_bubble(&self) -> Option<usize> {
        let slot = self.bubble_empty.filter(|_| self.is_bubbles() && matches!(self.mode, Mode::Home))?;
        let total = self.bubble_layout.as_ref().map_or(0, |l| l.centers.len());
        (slot < total && self.bubble_occupant(slot).is_none()).then_some(slot)
    }

    pub(super) fn navigate_bubbles(&mut self, action: Action) {
        let from = self.empty_bubble().unwrap_or_else(|| self.bubble_places().get(self.selected).copied().unwrap_or(self.selected));
        let to = match self.bubble_layout.as_ref() {
            Some(layout) if layout.centers.len() >= self.tile_count() => navigate_free(&layout.centers, from, action),
            _ => return,
        };
        if to == from {
            return;
        }
        match self.bubble_occupant(to) {
            Some(i) => {
                self.selected = i;
                self.bubble_empty = None;
            }
            None => self.bubble_empty = Some(to),
        }
        self.bubble_sound(to);
    }

    /// Bigger bubbles sound lower.
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

    pub(super) fn bubble_key(&self, i: usize) -> String {
        match self.library.games.get(i) {
            Some(game) => game.id.clone(),
            None => {
                let special = self.special(i);
                format!("special-{}", Special::ALL.iter().position(|s| Some(*s) == special).unwrap_or(0))
            }
        }
    }

    pub(super) fn update_video(&mut self, ctx: &Context) {
        let theme = &self.theme().settings;
        let wanted = (theme.kind == ThemeKind::Bubbles
            && theme.bubbles.play_video
            && matches!(self.mode, Mode::Home)
            && self.empty_bubble().is_none()
            && ctx.input(|i| i.focused))
        .then(|| self.selected_game().and_then(|g| g.asset(AssetKind::Video).map(Path::to_path_buf)))
        .flatten();
        if wanted != self.video_wanted.0 {
            self.video_wanted = (wanted.clone(), Instant::now());
        }
        if self.video.as_ref().map(VideoPlayer::path) != wanted.as_deref() {
            self.video = None;
        }
        let Some(path) = wanted else { return };
        if self.video.is_some() || self.video_failed {
            return;
        }
        let waited = self.video_wanted.1.elapsed();
        if waited < VIDEO_DELAY {
            ctx.request_repaint_after(VIDEO_DELAY - waited);
            return;
        }
        match VideoPlayer::start(ctx, &path) {
            Ok(player) => self.video = Some(player),
            Err(e) => {
                self.video_failed = true;
                self.notify(tr!("Videos need ffmpeg installed: {error}", error = format!("{e:#}")));
            }
        }
    }

    pub(super) fn draw_bubbles(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let count = self.tile_count();
        let bubbles = self.theme().settings.bubbles.clone();
        let top = self.bar_rect(BarSlot::Top, screen, s).bottom() + 16.0 * s;
        let area = Rect::from_min_max(pos2(screen.left() + 60.0 * s, top), pos2(screen.right() - 60.0 * s, screen.bottom() - 220.0 * s));
        let mut keys: Vec<String> = (0..count).map(|i| self.bubble_key(i)).collect();
        if bubbles.fill_screen {
            let base = bubbles.size.max(BubbleSettings::SIZE_RANGE.0) * s;
            let fits = (area.area() * 0.5 / (std::f32::consts::FRAC_PI_4 * base * base)) as usize;
            keys.extend((0..fits.saturating_sub(count)).map(|k| format!("empty-{k}")));
        }
        let diameters: Vec<f32> = keys.iter().map(|key| bubbles.diameter(key) * s).collect();
        if self.bubble_layout.as_ref().is_none_or(|c| c.diameters != diameters || c.area != area) {
            let (centers, radii) = pack_bubbles(&diameters, area);
            self.bubble_layout = Some(BubbleLayout { diameters, area, centers, radii });
        }
        let (centers, radii) = self.bubble_layout.as_ref().map(|l| (l.centers.clone(), l.radii.clone())).unwrap_or_default();
        if centers.len() < count {
            return;
        }
        let places = self.bubble_places();
        let slot = |i: usize| places.get(i).copied().unwrap_or(i);
        let mut occupant: Vec<Option<usize>> = vec![None; centers.len()];
        for i in 0..count {
            occupant[slot(i)] = Some(i);
        }

        let ctx = ui.ctx().clone();
        let video = match &mut self.video {
            Some(player) if bubbles.play_video => player.texture(&ctx).map(|t| (t.id(), t.size_vec2())),
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
        let mut clicked = None;
        let order = (0..centers.len())
            .filter(|&j| occupant[j].is_none() && j != focused && Some(j) != moving)
            .chain((0..centers.len()).filter(|&j| occupant[j].is_some() && j != focused && Some(j) != moving))
            .chain(moving.filter(|&j| j != focused))
            .chain([focused]);
        for j in order {
            let tile = occupant[j];
            let focus = ctx.animate_bool_with_time(Id::new(("bubble-focus", j)), j == focused, 0.18);
            let phase = tile.unwrap_or(j) as f32 * 1.7;
            let center = centers[j] + vec2((t * 0.5 + phase).cos() * 3.0 * s, (t * 0.7 + phase).sin() * 4.0 * s);
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
                swapping: Some(j) == moving,
                running: tile.is_some_and(|i| self.is_active(i)),
            };
            paint_bubble(ui.painter(), center, r, &look, t, focus, s);
            if interactive {
                let resp = ui.interact(Rect::from_center_size(center, Vec2::splat(r * 2.0)), Id::new(("bubble", j)), Sense::click());
                if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| p.distance(center) <= r) {
                    clicked = Some(j);
                }
            }
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

    pub(super) fn bubble_places(&self) -> Vec<usize> {
        let games = &self.library.games;
        let (n, count) = (games.len(), self.tile_count());
        let total = self.bubble_layout.as_ref().map_or(count, |l| l.centers.len()).max(count);
        let index: std::collections::HashMap<&str, usize> = games.iter().enumerate().map(|(i, g)| (g.id.as_str(), i)).collect();
        let slot_of = |key: &str| match key.strip_prefix("empty-") {
            Some(k) => k.parse::<usize>().ok().map(|k| count + k).filter(|&j| j < total),
            None => index.get(key).copied(),
        };
        let mut place = vec![usize::MAX; n];
        let mut taken = vec![false; total];
        for (i, game) in games.iter().enumerate() {
            if let Some(slot) = game.bubble_place.as_deref().and_then(slot_of)
                && !taken[slot]
            {
                place[i] = slot;
                taken[slot] = true;
            }
        }
        for i in 0..n {
            if place[i] == usize::MAX && !taken[i] {
                place[i] = i;
                taken[i] = true;
            }
        }
        let mut free = (0..n).chain(count..total).filter(|&j| !taken[j]);
        for p in place.iter_mut().filter(|p| **p == usize::MAX) {
            *p = free.next().unwrap_or(0);
        }
        place
    }

    pub(super) fn slot_key(&self, slot: usize) -> Option<String> {
        let (n, count) = (self.library.games.len(), self.tile_count());
        match slot {
            _ if slot < n => Some(self.library.games[slot].id.clone()),
            _ if slot >= count => Some(format!("empty-{}", slot - count)),
            _ => None,
        }
    }

    pub(super) fn move_bubble(&mut self, from: usize, target: usize) {
        let Some(target_key) = self.slot_key(target) else {
            self.notify(tr!("Only games can swap places"));
            return;
        };
        self.mode = Mode::Home;
        let places = self.bubble_places();
        let Some(&own) = places.get(from) else { return };
        if own == target {
            return;
        }
        let mut moves = vec![(from, target_key)];
        if let Some(other) = places.iter().position(|&p| p == target)
            && let Some(own_key) = self.slot_key(own)
        {
            moves.push((other, own_key));
        }
        for (i, key) in moves {
            let game = &mut self.library.games[i];
            game.bubble_place = (key != game.id).then_some(key);
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
                let sub = match game.last_played.and_then(|t| chrono::DateTime::from_timestamp(t as i64, 0)) {
                    Some(t) => tr!("Last played {date}", date = t.with_timezone(&chrono::Local).format(tr!("%d %b %Y, %H:%M"))),
                    None => tr!("Never played").to_owned(),
                };
                let sub = match self.artwork.status(&game.id) {
                    Some(metadata::Status::Done(at, _)) if at.elapsed() >= metadata::DONE_BADGE => sub,
                    Some(status) => format!("{sub}  ·  {}", status.describe()),
                    None => sub,
                };
                let sub = if self.is_active(self.selected) {
                    tr!("Running  ·  A switches to it, Guide comes back here").to_owned()
                } else if let Some(update) = self.game_update(game) {
                    tr!("{status}  ·  Update to {version} in Options", status = sub, version = update.latest.version)
                } else {
                    sub
                };
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
                let sub = match special {
                    Special::Store => tr!("Apps, games and themes from the YASAFF Store, installed and updated right here"),
                    Special::Add => tr!("Scan for installed games, or add an app, a .desktop/shortcut file, or a command or URL"),
                    Special::Settings => tr!("Appearance and themes, the YASAFF Store, storefronts (Heroic, Steam) and general options"),
                };
                let title = special.name();
                shadow_text(painter, pos2(x, y), Align2::LEFT_TOP, title, 46.0 * s, Color32::WHITE);
                shadow_text(painter, pos2(x, y + 62.0 * s), Align2::LEFT_TOP, sub, 22.0 * s, Color32::from_gray(190));
            }
        }
        clicked
    }

    pub(super) fn click_tile(&mut self, i: usize) {
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

    pub(super) fn handle_swap_bubble(&mut self, action: Action) {
        let Mode::SwapBubble { from, target } = &mut self.mode else { return };
        match action {
            Action::Left | Action::Right | Action::Up | Action::Down => {
                let from = *target;
                let to = navigate_free(self.bubble_layout.as_ref().map_or(&[], |l| &l.centers), from, action);
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

#[cfg(test)]
mod tests {
    use super::{Action, Layout, navigate, navigate_free, pack_bubbles};
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
