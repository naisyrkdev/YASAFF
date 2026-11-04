//! Adding games: the Add menu, the installed-apps picker, the library scan and artwork fetching.

use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum AddSource {
    Scan,
    Installed,
    Browse,
    Command,
    ImportApp,
    Github,
}

pub(super) const ADD_MENU: [(AddSource, &str, &str); 6] = [
    (AddSource::Scan, tr_key!("Scan for games"), tr_key!("Finds installed Steam and Heroic games and game shortcuts")),
    (AddSource::Installed, tr_key!("Installed applications"), tr_key!("Games, desktop shortcuts (Heroic, Steam, Lutris…) and apps")),
    (AddSource::Browse, tr_key!("Browse for file…"), tr_key!("Executable, script, AppImage, .desktop or shortcut")),
    (AddSource::Command, tr_key!("Command or URL…"), tr_key!("e.g. steam://rungameid/570 or a full command line")),
    (AddSource::ImportApp, tr_key!("Import application from file…"), tr_key!("A YASAFF app or addon package: a .zip with yasaffConfig.json")),
    (AddSource::Github, tr_key!("Install from GitHub…"), tr_key!("A YASAFF app or addon from a GitHub repository, as owner/name")),
];

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Tab {
    Games,
    Desktop,
    All,
}

impl Tab {
    pub(super) fn label(self) -> &'static str {
        match self {
            Tab::Games => tr!("Games"),
            Tab::Desktop => tr!("Desktop shortcuts"),
            Tab::All => tr!("All applications"),
        }
    }

    pub(super) fn matches(self, app: &AppEntry) -> bool {
        match self {
            Tab::Games => app.is_game,
            Tab::Desktop => app.on_desktop,
            Tab::All => true,
        }
    }
}

pub(super) struct Picker {
    pub(super) apps: Vec<AppEntry>,
    pub(super) tabs: Vec<Tab>,
    pub(super) tab: usize,
    pub(super) cursor: usize,
    pub(super) scroll: usize,
    pub(super) filter: String,
}

impl Picker {
    pub(super) fn new() -> Self {
        let apps = apps::installed();
        let tabs = [Tab::Games, Tab::Desktop, Tab::All]
            .into_iter()
            .filter(|t| *t == Tab::All || apps.iter().any(|a| t.matches(a)))
            .collect();
        Picker { apps, tabs, tab: 0, cursor: 0, scroll: 0, filter: String::new() }
    }

    pub(super) fn visible(&self) -> Vec<usize> {
        let tab = self.tabs[self.tab];
        let filter = self.filter.to_lowercase();
        (0..self.apps.len())
            .filter(|&i| tab.matches(&self.apps[i]) && self.apps[i].name.to_lowercase().contains(&filter))
            .collect()
    }
}

pub(super) struct ScanState {
    pub(super) pending: Option<mpsc::Receiver<(Vec<scan::Found>, usize)>>,
    pub(super) found: Vec<scan::Found>,
    pub(super) checked: Vec<bool>,
    pub(super) in_library: usize,
    pub(super) assets_rx: Option<mpsc::Receiver<(usize, scan::Assets)>>,
    pub(super) assets: Vec<Option<FoundAssets>>,
    pub(super) fetch_artwork: bool,
    pub(super) first_run: bool,
    pub(super) cursor: usize,
    pub(super) scroll: usize,
}

pub(super) struct FoundAssets {
    pub(super) cover: bool,
    pub(super) background: bool,
    pub(super) thumbnail: Option<TextureHandle>,
}

impl ScanState {
    pub(super) fn start(ctx: &Context, known: scan::Known, fetch_artwork: bool, first_run: bool) -> Self {
        let (tx, pending) = mpsc::channel();
        let (assets_tx, assets_rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut found = scan::scan();
            let total = found.len();
            found.retain(|f| !known.has(f));
            let entries: Vec<AppEntry> = found.iter().map(|f| f.entry.clone()).collect();
            let sent = tx.send((found, total)).is_ok();
            ctx.request_repaint();
            if !sent {
                return;
            }
            for (i, entry) in entries.iter().enumerate() {
                if assets_tx.send((i, scan::assets(entry))).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
        ScanState {
            pending: Some(pending),
            found: Vec::new(),
            checked: Vec::new(),
            in_library: 0,
            assets_rx: Some(assets_rx),
            assets: Vec::new(),
            fetch_artwork,
            first_run,
            cursor: 0,
            scroll: 0,
        }
    }

    pub(super) fn fetch_row(&self) -> usize {
        self.found.len()
    }

    pub(super) fn import_row(&self) -> usize {
        self.found.len() + 1
    }

    pub(super) fn ticked(&self) -> usize {
        self.checked.iter().filter(|c| **c).count()
    }
}

pub(super) fn add_menu_index(source: AddSource) -> usize {
    ADD_MENU.iter().position(|(s, ..)| *s == source).unwrap_or(0)
}

pub(super) const COMMAND_HELP: &str = tr_key!("Examples:  steam://rungameid/570   ·   wine \"/games/TQ/TQ.exe\"   ·   flatpak run com.example.Game\n\
     Quote arguments that contain spaces. URIs are opened by the system handler.");

impl App {
    pub(super) fn run_add_source(&mut self, source: AddSource) {
        match source {
            AddSource::Scan => {
                self.mode = Mode::Scan(Box::new(ScanState::start(&self.ctx, scan::Known::of(&self.library), true, false)));
            }
            AddSource::Installed => self.mode = Mode::Picker(Box::new(Picker::new())),
            AddSource::Browse => {
                let dialog = rfd::FileDialog::new().set_title(tr!("Select a game executable, script or shortcut"));
                self.ask_path(dialog, Pick::File, |app, path| app.import_entry(apps::from_file(&path)));
            }
            AddSource::Command => {
                self.mode = Mode::Prompt { kind: PromptKind::AddCommand, buffer: String::new() };
            }
            AddSource::ImportApp => self.import_app_file(),
            AddSource::Github => self.ask_github_repo(String::new()),
        }
    }

    pub(super) fn import_entry(&mut self, entry: AppEntry) {
        if let Some(i) = self.library.find(&entry.target, &entry.args) {
            self.selected = i;
            self.mode = Mode::Home;
            self.notify(tr!("{name} is already in your library", name = self.library.games[i].name));
            return;
        }
        let i = self.add_entry(&entry);
        self.selected = i;
        self.save();
        if self.library.games[i].asset(AssetKind::Cover).is_some() {
            self.mode = Mode::Home;
            self.notify(tr!("Added {name}", name = entry.name));
        } else {
            self.mode = Mode::Options { cursor: self.menu_index(MenuItem::Appearance) };
        }
    }

    pub(super) fn add_entry(&mut self, entry: &AppEntry) -> usize {
        let i = self.library.add(entry);
        self.library.games[i].save_dir = crate::saves::detect(&self.library.games[i]);
        if let Some(icon) = &entry.icon
            && let Err(e) = self.library.set_asset(i, AssetKind::Cover, icon)
        {
            self.notify(tr!("Could not use the shortcut's icon: {error}", error = format!("{e:#}")));
        }
        i
    }

    pub(super) fn poll_scan(&mut self) {
        let Mode::Scan(sc) = &mut self.mode else { return };
        if let Some(rx) = &sc.pending {
            let (found, total) = match rx.try_recv() {
                Ok(listed) => listed,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => (Vec::new(), 0),
            };
            sc.in_library = total.saturating_sub(found.len());
            sc.checked = vec![true; found.len()];
            sc.assets = std::iter::repeat_with(|| None).take(found.len()).collect();
            sc.found = found;
            sc.pending = None;
            sc.cursor = 0;
            sc.scroll = 0;
        }
        let Some(rx) = &sc.assets_rx else { return };
        loop {
            match rx.try_recv() {
                Ok((i, assets)) => {
                    let thumbnail = assets.thumbnail.map(|image| self.ctx.load_texture(format!("scan-cover-{i}"), image, TextureOptions::LINEAR));
                    if let Some(slot) = sc.assets.get_mut(i) {
                        *slot = Some(FoundAssets { cover: assets.cover.is_some(), background: assets.background.is_some(), thumbnail });
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    sc.assets_rx = None;
                    break;
                }
            }
        }
    }

    pub(super) fn import_scanned(&mut self) {
        let Mode::Scan(sc) = std::mem::replace(&mut self.mode, Mode::Home) else { return };
        if sc.ticked() == 0 {
            self.mode = Mode::Scan(sc);
            return self.notify(tr!("Tick the games you want to import first"));
        }
        let mut added = Vec::new();
        for (found, _) in sc.found.iter().zip(&sc.checked).filter(|(_, checked)| **checked) {
            if self.library.find(&found.entry.target, &found.entry.args).is_none() {
                let i = self.add_entry(&found.entry);
                added.push(self.library.games[i].id.clone());
            }
        }
        self.save();
        if let Some(i) = added.first().and_then(|id| self.library.position(id)) {
            self.selected = i;
        }
        let n = added.len();
        self.notify(tr_n!(n, "Added {n} game", "Added {n} games"));
        if sc.fetch_artwork {
            self.fetch_artwork(&added);
        }
    }

    pub(super) fn fetch_artwork(&mut self, game_ids: &[String]) {
        for id in game_ids {
            let Some(game) = self.library.games.iter().find(|g| g.id == *id) else { continue };
            let request = metadata::Request { game_id: id.clone(), ids: game.external_ids() };
            self.artwork.fetch(request);
        }
    }

    pub(super) fn poll_artwork(&mut self) {
        for art in self.artwork.poll() {
            if let Some(i) = self.library.position(&art.game_id) {
                for (kind, path) in [(AssetKind::Cover, &art.cover), (AssetKind::Background, &art.background)] {
                    if let Some(path) = path
                        && let Err(e) = self.library.set_asset(i, kind, path)
                    {
                        self.notify(tr!("Could not use the artwork: {error}", error = format!("{e:#}")));
                    }
                }
                self.save();
            }
            for path in art.downloads {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    pub(super) fn draw_add_menu(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::AddMenu { cursor } = &mut self.mode else { return };
        let rows: Vec<_> = ADD_MENU.iter().map(|(_, label, sub)| (i18n::tr(label), Some(i18n::tr(sub)))).collect();
        if let Some(i) = side_panel(ui, screen, s, tr!("Add game"), &rows, &[], cursor) {
            self.run_add_source(ADD_MENU[i].0);
        }
    }

    pub(super) fn draw_picker(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::Picker(p) = &mut self.mode else { return };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(170));
        let panel = Rect::from_center_size(
            screen.center(),
            vec2((1100.0 * s).min(screen.width() - 40.0), screen.height() - 150.0 * s),
        );
        painter.rect_filled(panel, (8.0 * s) as u8, Color32::from_rgb(14, 20, 36));
        painter.text(panel.left_top() + vec2(40.0 * s, 36.0 * s), Align2::LEFT_TOP, tr!("Installed applications"), FontId::proportional(30.0 * s), Color32::WHITE);

        let mut x = panel.left() + 40.0 * s;
        let tab_y = panel.top() + 96.0 * s;
        let font = FontId::proportional(20.0 * s);
        let mut clicked_tab = None;
        for (i, tab) in p.tabs.iter().enumerate() {
            let active = i == p.tab;
            let text = if active { Color32::BLACK } else { Color32::WHITE };
            let galley = painter.layout_no_wrap(tab.label().to_owned(), font.clone(), text);
            let r = Rect::from_min_size(pos2(x, tab_y), galley.size() + vec2(32.0 * s, 16.0 * s));
            painter.rect_filled(r, (r.height() / 2.0) as u8, if active { Color32::WHITE } else { Color32::from_white_alpha(20) });
            painter.galley(r.center() - galley.size() / 2.0, galley, text);
            if ui.interact(r, Id::new(("tab", i)), Sense::click()).clicked() {
                clicked_tab = Some(i);
            }
            x = r.right() + 12.0 * s;
        }
        let search = if p.filter.is_empty() { tr!("Type to search…").to_owned() } else { tr!("Search: {text}|", text = p.filter) };
        painter.text(pos2(panel.right() - 40.0 * s, tab_y + 18.0 * s), Align2::RIGHT_CENTER, search, font.clone(), Color32::from_gray(170));

        let visible = p.visible();
        let row_h = 68.0 * s;
        let list_top = tab_y + 64.0 * s;
        let rows = (((panel.bottom() - 24.0 * s - list_top) / row_h).floor() as usize).max(1);
        let wheel = wheel_lines(ui);
        if wheel.abs() >= 0.5 {
            p.cursor = p.cursor.saturating_add_signed(-(wheel.round().clamp(-5.0, 5.0) as isize));
        }
        p.cursor = p.cursor.min(visible.len().saturating_sub(1));
        if p.cursor < p.scroll {
            p.scroll = p.cursor;
        } else if p.cursor >= p.scroll + rows {
            p.scroll = p.cursor + 1 - rows;
        }
        p.scroll = p.scroll.min(visible.len().saturating_sub(rows));

        if visible.is_empty() {
            painter.text(pos2(panel.center().x, list_top + 60.0 * s), Align2::CENTER_CENTER, tr!("Nothing found"), font.clone(), Color32::from_gray(150));
        }
        let mut activate = false;
        for (row, &app_i) in visible.iter().enumerate().skip(p.scroll).take(rows) {
            let app = &p.apps[app_i];
            let r = Rect::from_min_size(
                pos2(panel.left() + 20.0 * s, list_top + (row - p.scroll) as f32 * row_h),
                vec2(panel.width() - 40.0 * s, row_h - 6.0 * s),
            );
            if row == p.cursor {
                painter.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(22));
                painter.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
            }
            let icon = Rect::from_min_size(r.left_top() + vec2(20.0 * s, 6.0 * s), vec2(r.height() - 12.0 * s, r.height() - 12.0 * s));
            match &app.icon {
                Some(path) => paint_image_cover(ui, path, icon, 1.0, (4.0 * s) as u8),
                None => {
                    painter.rect_filled(icon, (4.0 * s) as u8, Color32::from_white_alpha(18));
                    let initial = app.name.chars().next().unwrap_or('?').to_uppercase().to_string();
                    painter.text(icon.center(), Align2::CENTER_CENTER, initial, font.clone(), Color32::WHITE);
                }
            }
            let text_x = icon.right() + 18.0 * s;
            let clip = painter.with_clip_rect(r.shrink(2.0));
            clip.text(pos2(text_x, r.top() + 8.0 * s), Align2::LEFT_TOP, &app.name, FontId::proportional(22.0 * s), Color32::WHITE);
            let command = apps::join_command(&app.target, &app.args);
            clip.text(pos2(text_x, r.top() + 38.0 * s), Align2::LEFT_TOP, command, FontId::proportional(15.0 * s), Color32::from_gray(140));
            if self.library.find(&app.target, &app.args).is_some() {
                let tag = painter.layout_no_wrap(tr!("In library").to_owned(), FontId::proportional(15.0 * s), Color32::BLACK);
                let t = Rect::from_min_size(pos2(r.right() - 20.0 * s - tag.size().x - 20.0 * s, r.center().y - 13.0 * s), tag.size() + vec2(20.0 * s, 10.0 * s));
                painter.rect_filled(t, (t.height() / 2.0) as u8, PAD_A);
                painter.galley(t.center() - tag.size() / 2.0, tag, Color32::BLACK);
            }

            let resp = ui.interact(r, Id::new(("app", app_i)), Sense::click());
            if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                p.cursor = row;
            }
            if resp.clicked() {
                activate = row == p.cursor;
                p.cursor = row;
            }
        }

        if let Some(tab) = clicked_tab {
            p.tab = tab;
            p.cursor = 0;
        }
        if activate {
            self.handle(ui.ctx(), Action::Confirm);
        }
    }

    pub(super) fn draw_scan(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::Scan(sc) = &mut self.mode else { return };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(170));
        let panel = Rect::from_center_size(
            screen.center(),
            vec2((1100.0 * s).min(screen.width() - 40.0), screen.height() - 150.0 * s),
        );
        painter.rect_filled(panel, (8.0 * s) as u8, Color32::from_rgb(14, 20, 36));
        let title = if sc.first_run { tr!("Welcome! Pick the games to add") } else { tr!("Scan for games") };
        painter.text(panel.left_top() + vec2(40.0 * s, 36.0 * s), Align2::LEFT_TOP, title, FontId::proportional(30.0 * s), Color32::WHITE);

        let font = FontId::proportional(20.0 * s);
        let gray = Color32::from_gray(170);
        let status_pos = pos2(panel.left() + 40.0 * s, panel.top() + 104.0 * s);
        if sc.pending.is_some() {
            let spinner = Rect::from_center_size(status_pos + vec2(12.0 * s, 0.0), vec2(24.0 * s, 24.0 * s));
            egui::Spinner::new().size(24.0 * s).color(Color32::WHITE).paint_at(ui, spinner);
            let text = tr!("Looking in Steam, Heroic and game shortcuts…");
            painter.text(status_pos + vec2(40.0 * s, 0.0), Align2::LEFT_CENTER, text, font.clone(), gray);
        } else {
            let n = sc.found.len();
            let mut text = tr_n!(n, "Found {n} new game", "Found {n} new games");
            if sc.in_library > 0 {
                text += &tr!("  ·  {count} already in your library", count = sc.in_library);
            }
            painter.text(status_pos, Align2::LEFT_CENTER, text, font.clone(), gray);
        }

        let row_h = 92.0 * s;
        let footer_h = 68.0 * s;
        let footer_top = panel.bottom() - 24.0 * s - 2.0 * footer_h;
        let list_top = status_pos.y + 36.0 * s;
        let rows = (((footer_top - 12.0 * s - list_top) / row_h).floor() as usize).max(1);
        let wheel = wheel_lines(ui);
        if wheel.abs() >= 0.5 {
            sc.cursor = sc.cursor.saturating_add_signed(-(wheel.round().clamp(-5.0, 5.0) as isize));
        }
        sc.cursor = sc.cursor.min(sc.import_row());
        if sc.cursor < sc.found.len() {
            if sc.cursor < sc.scroll {
                sc.scroll = sc.cursor;
            } else if sc.cursor >= sc.scroll + rows {
                sc.scroll = sc.cursor + 1 - rows;
            }
        }
        sc.scroll = sc.scroll.min(sc.found.len().saturating_sub(rows));

        if sc.pending.is_none() && sc.found.is_empty() {
            let text = tr!("Nothing new to import. Install a game, or press Menu / Tab to scan again.");
            painter.text(pos2(panel.center().x, list_top + 60.0 * s), Align2::CENTER_CENTER, text, font.clone(), Color32::from_gray(150));
        }
        let highlight = |r: Rect| {
            painter.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(22));
            painter.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
        };
        let row_rect = |top: f32, h: f32| Rect::from_min_size(pos2(panel.left() + 20.0 * s, top), vec2(panel.width() - 40.0 * s, h - 6.0 * s));
        let checkbox_rect = |r: Rect| Rect::from_center_size(pos2(r.left() + 36.0 * s, r.center().y), vec2(26.0 * s, 26.0 * s));
        let mut hovered = None;
        let mut clicked = None;

        for (row, found) in sc.found.iter().enumerate().skip(sc.scroll).take(rows) {
            let r = row_rect(list_top + (row - sc.scroll) as f32 * row_h, row_h);
            if row == sc.cursor {
                highlight(r);
            }
            let check = checkbox_rect(r);
            paint_checkbox(painter, check, sc.checked[row], s);
            let cover_h = r.height() - 12.0 * s;
            let icon = Rect::from_min_size(pos2(check.right() + 20.0 * s, r.top() + 6.0 * s), vec2(cover_h * 2.0 / 3.0, cover_h));
            let assets = sc.assets.get(row).and_then(Option::as_ref);
            match (assets.and_then(|a| a.thumbnail.as_ref()), &found.entry.icon) {
                (Some(thumbnail), _) => paint_placed(ui, egui::Image::new(thumbnail), icon, 1.0, (4.0 * s) as u8, CoverPlacement::default()),
                (None, Some(path)) => paint_image_cover(ui, path, icon, 1.0, (4.0 * s) as u8),
                (None, None) => {
                    painter.rect_filled(icon, (4.0 * s) as u8, placeholder_color(&found.entry.name));
                    painter.text(icon.center(), Align2::CENTER_CENTER, initials(&found.entry.name), FontId::proportional(icon.height() * 0.35), Color32::WHITE);
                }
            }

            let tag = painter.layout_no_wrap(found.source.clone(), FontId::proportional(15.0 * s), Color32::WHITE);
            let t = Rect::from_min_size(pos2(r.right() - 20.0 * s - tag.size().x - 20.0 * s, r.center().y - 13.0 * s), tag.size() + vec2(20.0 * s, 10.0 * s));
            painter.rect_filled(t, (t.height() / 2.0) as u8, Color32::from_white_alpha(30));
            painter.galley(t.center() - tag.size() / 2.0, tag, Color32::WHITE);

            let text_x = icon.right() + 18.0 * s;
            let clip = painter.with_clip_rect(Rect::from_min_max(r.min, pos2(t.left() - 12.0 * s, r.bottom())));
            clip.text(pos2(text_x, r.top() + 8.0 * s), Align2::LEFT_TOP, &found.entry.name, FontId::proportional(22.0 * s), Color32::WHITE);
            let location = tr!("Found on: {path}", path = found.location.display());
            clip.text(pos2(text_x, r.top() + 36.0 * s), Align2::LEFT_TOP, location, FontId::proportional(15.0 * s), Color32::from_gray(140));
            let chip_font = FontId::proportional(14.0 * s);
            let chip_top = r.top() + 60.0 * s;
            match assets {
                None => {
                    clip.text(pos2(text_x, chip_top + 2.0 * s), Align2::LEFT_TOP, tr!("Looking for images…"), chip_font, Color32::from_gray(110));
                }
                Some(assets) => {
                    let mut x = text_x;
                    for (label, has, used) in [(tr!("Cover"), assets.cover, sc.fetch_artwork || found.entry.icon.is_some()), (tr!("Background"), assets.background, sc.fetch_artwork)] {
                        let (mark, color) = match (has, used) {
                            (true, true) => ("✔", PAD_A),
                            (true, false) => ("✔", Color32::from_gray(120)),
                            (false, _) => ("✘", Color32::from_gray(90)),
                        };
                        let chip = clip.layout_no_wrap(format!("{mark} {label}"), chip_font.clone(), color);
                        let rect = Rect::from_min_size(pos2(x, chip_top), chip.size() + vec2(14.0 * s, 6.0 * s));
                        clip.rect_stroke(rect, (rect.height() / 2.0) as u8, Stroke::new(1.0, color.gamma_multiply(0.6)), StrokeKind::Inside);
                        clip.galley(rect.center() - chip.size() / 2.0, chip, color);
                        x = rect.right() + 8.0 * s;
                    }
                }
            }

            let resp = ui.interact(r, Id::new(("scan", row)), Sense::click());
            if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                hovered = Some(row);
            }
            if resp.clicked() {
                clicked = Some(row);
            }
        }

        painter.hline(panel.left() + 20.0 * s..=panel.right() - 20.0 * s, footer_top - 8.0 * s, Stroke::new(1.0, Color32::from_white_alpha(25)));
        let fetch = row_rect(footer_top, footer_h);
        if sc.cursor == sc.fetch_row() {
            highlight(fetch);
        }
        let check = checkbox_rect(fetch);
        paint_checkbox(painter, check, sc.fetch_artwork, s);
        let x = check.right() + 20.0 * s;
        painter.text(pos2(x, fetch.center().y - 10.0 * s), Align2::LEFT_BOTTOM, tr!("Fetch metadata"), FontId::proportional(22.0 * s), Color32::WHITE);
        let with_images = sc
            .assets
            .iter()
            .zip(&sc.checked)
            .filter(|(a, checked)| **checked && a.as_ref().is_some_and(|a| a.cover || a.background))
            .count();
        let note = if sc.assets_rx.is_some() {
            tr!("Looking for images on this computer…").to_owned()
        } else if with_images == 0 {
            tr!("No images found on this computer for the ticked games").to_owned()
        } else {
            tr_n!(with_images, "Use the images found on this computer for {n} ticked game", "Use the images found on this computer for {n} ticked games")
        };
        painter.text(pos2(x, fetch.center().y - 6.0 * s), Align2::LEFT_TOP, note, FontId::proportional(15.0 * s), Color32::from_gray(140));

        let import_row = row_rect(footer_top + footer_h, footer_h);
        let ticked = sc.ticked();
        let label = if sc.pending.is_some() {
            tr!("Import").to_owned()
        } else {
            tr_n!(ticked, "Import {n} game", "Import {n} games")
        };
        let btn = Rect::from_min_size(pos2(import_row.left(), import_row.top() + 4.0 * s), vec2(320.0 * s, import_row.height() - 8.0 * s));
        let focused = sc.cursor == sc.import_row();
        let usable = ticked > 0 && sc.pending.is_none();
        let (fill, text) = match (focused, usable) {
            (true, true) => (Color32::WHITE, Color32::BLACK),
            (true, false) => (Color32::from_gray(120), Color32::from_gray(40)),
            (false, _) => (Color32::from_white_alpha(25), if usable { Color32::WHITE } else { Color32::from_gray(120) }),
        };
        painter.rect_filled(btn, (4.0 * s) as u8, fill);
        painter.text(btn.center(), Align2::CENTER_CENTER, label, FontId::proportional(22.0 * s), text);

        for (r, row) in [(fetch, sc.fetch_row()), (btn, sc.import_row())] {
            let resp = ui.interact(r, Id::new(("scan-footer", row == sc.fetch_row())), Sense::click());
            if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                hovered = Some(row);
            }
            if resp.clicked() {
                clicked = Some(row);
            }
        }

        if let Some(row) = hovered {
            sc.cursor = row;
        }
        if let Some(row) = clicked {
            sc.cursor = row;
            self.handle(ui.ctx(), Action::Confirm);
        }
    }
}

impl App {
    pub(super) fn handle_add_menu(&mut self, action: Action) {
        let Mode::AddMenu { cursor } = &mut self.mode else { return };
        match action {
            Action::Up => *cursor = cursor.saturating_sub(1),
            Action::Down => *cursor = (*cursor + 1).min(ADD_MENU.len() - 1),
            Action::Confirm => {
                let source = ADD_MENU[*cursor].0;
                self.run_add_source(source);
            }
            Action::Back => self.mode = Mode::Home,
            _ => {}
        }
    }

    pub(super) fn handle_picker(&mut self, action: Action) {
        let Mode::Picker(p) = &mut self.mode else { return };
        match action {
            Action::Up => p.cursor = p.cursor.saturating_sub(1),
            Action::Down => p.cursor += 1,
            Action::Left | Action::Right => {
                p.tab = match action {
                    Action::Left => p.tab.saturating_sub(1),
                    _ => (p.tab + 1).min(p.tabs.len() - 1),
                };
                p.cursor = 0;
            }
            Action::Type(c) => {
                p.filter.push(c);
                p.cursor = 0;
            }
            Action::Erase => {
                p.filter.pop();
                p.cursor = 0;
            }
            Action::Confirm => {
                if let Some(&i) = p.visible().get(p.cursor) {
                    let entry = p.apps[i].clone();
                    self.import_entry(entry);
                }
            }
            Action::Back if !p.filter.is_empty() => p.filter.clear(),
            Action::Back => self.mode = Mode::AddMenu { cursor: add_menu_index(AddSource::Installed) },
            _ => {}
        }
    }

    pub(super) fn handle_scan(&mut self, ctx: &Context, action: Action) {
        let Mode::Scan(sc) = &mut self.mode else { return };
        match action {
            Action::Up => sc.cursor = sc.cursor.saturating_sub(1),
            Action::Down => sc.cursor = (sc.cursor + 1).min(sc.import_row()),
            Action::Confirm if sc.pending.is_some() => {}
            Action::Confirm => {
                if sc.cursor < sc.found.len() {
                    sc.checked[sc.cursor] ^= true;
                } else if sc.cursor == sc.fetch_row() {
                    sc.fetch_artwork ^= true;
                } else {
                    self.import_scanned();
                }
            }
            Action::Add => {
                let all = sc.checked.iter().all(|c| *c);
                sc.checked.fill(!all);
            }
            Action::Options => **sc = ScanState::start(ctx, scan::Known::of(&self.library), sc.fetch_artwork, sc.first_run),
            Action::Back if sc.first_run => self.mode = Mode::Welcome { cursor: 0 },
            Action::Back => self.mode = Mode::AddMenu { cursor: add_menu_index(AddSource::Scan) },
            _ => {}
        }
    }
}
