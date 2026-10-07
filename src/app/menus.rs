//! The game Options menu, confirmation dialogs and the text prompt.

use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum MenuItem {
    Start,
    CloseApp,
    Update,
    SetCover,
    AdjustCover,
    SetBackground,
    FetchArtwork,
    BoxAppearance,
    Rename,
    SwapBubble,
    SwapTile,
    FixSlot,
    EditCommand,
    SaveFolder,
    OpenSaveFolder,
    BackupSaves,
    DeviceSaves,
    CreateDisc,
    BurnDisc,
    Remove,
}

pub(super) const MENU: [(MenuItem, &str); 16] = [
    (MenuItem::Start, tr_key!("Start")),
    (MenuItem::BoxAppearance, tr_key!("Tile and box appearance…")),
    (MenuItem::SetCover, tr_key!("Set cover image")),
    (MenuItem::AdjustCover, tr_key!("Reposition cover image…")),
    (MenuItem::SetBackground, tr_key!("Set background image")),
    (MenuItem::FetchArtwork, tr_key!("Fetch artwork")),
    (MenuItem::Rename, tr_key!("Rename")),
    (MenuItem::SwapBubble, tr_key!("Swap with other bubble")),
    (MenuItem::SwapTile, tr_key!("Swap position with another game…")),
    (MenuItem::FixSlot, tr_key!("Mark this position as static for this game")),
    (MenuItem::EditCommand, tr_key!("Edit launch command")),
    (MenuItem::SaveFolder, tr_key!("Set save folder…")),
    (MenuItem::DeviceSaves, tr_key!("SD and USB Yasaff settings…")),
    (MenuItem::CreateDisc, tr_key!("Create disc file…")),
    (MenuItem::BurnDisc, tr_key!("Burn to CD/DVD…")),
    (MenuItem::Remove, tr_key!("Remove from library")),
];

#[derive(Clone, Copy, PartialEq)]
pub(super) enum PromptKind {
    Rename,
    AddCommand,
    EditCommand,
    StoreCommand(Store),
    ThemeName,
    AppStoreUrl,
    Color(SettingRow),
    BoxSize(SettingRow),
    CrtSize(SettingRow),
    ProfileName,
    ProfileNew,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Removal {
    Game,
    Theme,
    AppStore,
    Addon(usize),
    /// Index into `crate::appstore::addon_packs()`.
    AddonPack(usize),
    Profile,
    Everything,
    /// Not a removal: turning on static order, which deserves the same second look.
    StaticOrder,
}

impl App {
    pub(super) fn menu(&self) -> Vec<(MenuItem, String)> {
        let game = self.selected_game();
        let store_app = game.is_some_and(|g| g.app_id.is_some());
        let running = store_app && self.app_running_at(self.selected);
        let game_running = !store_app && self.game_index() == Some(self.selected);
        let boxes = self.theme().settings.tile_style.is_box();
        let mut menu = Vec::new();
        for &(item, label) in &MENU {
            match item {
                MenuItem::Start if running => {
                    menu.push((item, tr!("Switch to app").to_owned()));
                    menu.push((MenuItem::CloseApp, tr!("Close application").to_owned()));
                }
                MenuItem::Start if game_running => {
                    menu.push((item, tr!("Switch to game").to_owned()));
                    menu.push((MenuItem::CloseApp, tr!("Close game").to_owned()));
                }
                MenuItem::SaveFolder | MenuItem::DeviceSaves if store_app => {}
                MenuItem::SwapBubble if !self.is_bubbles() => {}
                MenuItem::SwapTile if self.is_bubbles() || self.library.games.len() < 2 => {}
                MenuItem::FixSlot if self.is_bubbles() || self.settings.order.static_order => {}
                MenuItem::FixSlot if game.is_some_and(|g| g.fixed_slot.is_some()) => {
                    menu.push((item, tr!("Unmark static position (slot {slot})", slot = self.selected + 1)));
                }
                MenuItem::FixSlot => menu.push((item, tr!("Mark this position as static for this game (slot {slot})", slot = self.selected + 1))),
                MenuItem::SetCover if boxes => {}
                MenuItem::AdjustCover if boxes || game.is_none_or(|g| g.asset(AssetKind::Cover).is_none()) => {}
                MenuItem::EditCommand | MenuItem::FetchArtwork if store_app => {}
                MenuItem::Remove if store_app => menu.push((item, tr!("Uninstall").to_owned())),
                _ => menu.push((item, i18n::tr(label).to_owned())),
            }
            if item == MenuItem::Start
                && let Some(update) = game.and_then(|g| self.game_update(g))
            {
                menu.push((MenuItem::Update, tr!("Update to {version}", version = update.latest.version)));
            }
            if item == MenuItem::SaveFolder && game.is_some_and(|g| g.save_dir.is_some()) {
                menu.push((MenuItem::OpenSaveFolder, tr!("Open save folder").to_owned()));
                menu.push((MenuItem::BackupSaves, tr!("Back up saves…").to_owned()));
            }
        }
        menu
    }

    pub(super) fn menu_index(&self, item: MenuItem) -> usize {
        self.menu().iter().position(|(i, _)| *i == item).unwrap_or(0)
    }

    pub(super) fn run_menu_item(&mut self, item: MenuItem) {
        let Some(game) = self.selected_game() else { return };
        match item {
            MenuItem::Start => {
                self.mode = Mode::Home;
                self.launch_selected();
            }
            MenuItem::CloseApp if game.app_id.is_some() => self.close_running(),
            MenuItem::CloseApp => self.close_game(),
            MenuItem::SaveFolder => self.pick_save_dir(),
            MenuItem::OpenSaveFolder => self.open_save_dir(),
            MenuItem::BackupSaves => self.backup_saves(),
            MenuItem::DeviceSaves => self.open_device_settings(),
            MenuItem::Update => self.update_selected(),
            MenuItem::SetCover => self.pick_asset(AssetKind::Cover),
            MenuItem::AdjustCover => self.open_cover_editor(AssetKind::Cover, true),
            MenuItem::SetBackground => self.pick_asset(AssetKind::Background),
            MenuItem::FetchArtwork => {
                let id = game.id.clone();
                self.mode = Mode::Home;
                self.fetch_artwork(&[id]);
            }
            MenuItem::BoxAppearance => self.mode = Mode::Settings { cursor: 0, page: Page::GameBox, in_rows: false },
            MenuItem::Rename => {
                self.mode = Mode::Prompt { kind: PromptKind::Rename, buffer: game.name.clone() };
            }
            MenuItem::SwapBubble => {
                let target = self.bubble_places().get(self.selected).copied().unwrap_or(self.selected);
                self.mode = Mode::SwapBubble { from: self.selected, target };
            }
            MenuItem::SwapTile => self.mode = Mode::SwapTile { from: self.selected },
            MenuItem::FixSlot => {
                let fixed = game.fixed_slot.is_none();
                self.library.set_fixed(self.selected, fixed);
                self.save();
                let slot = self.selected + 1;
                self.notify(if fixed {
                    tr!("Stays in slot {slot}, whatever else moves", slot = slot)
                } else {
                    tr!("Slot {slot} is no longer static", slot = slot)
                });
                self.mode = Mode::Home;
            }
            MenuItem::EditCommand => {
                let buffer = apps::join_command(&game.target, &game.args);
                self.mode = Mode::Prompt { kind: PromptKind::EditCommand, buffer };
            }
            MenuItem::CreateDisc => self.create_disc_file(),
            MenuItem::BurnDisc => self.open_burner(),
            MenuItem::Remove => self.mode = Mode::ConfirmRemove { what: Removal::Game, yes: false },
        }
    }

    pub(super) fn commit_prompt(&mut self) {
        let Mode::Prompt { kind, buffer } = std::mem::replace(&mut self.mode, Mode::Home) else { return };
        let text = buffer.trim();
        match kind {
            PromptKind::Rename if !text.is_empty() => {
                self.library.games[self.selected].name = text.to_owned();
                self.save();
            }
            PromptKind::AddCommand => {
                if let Some(entry) = apps::from_command(text) {
                    self.import_entry(entry);
                }
            }
            PromptKind::EditCommand => {
                if let Some(entry) = apps::from_command(text) {
                    let game = &mut self.library.games[self.selected];
                    game.target = entry.target;
                    game.args = entry.args;
                    self.save();
                    self.notify(tr!("Launch command updated"));
                }
            }
            PromptKind::StoreCommand(store) => {
                let detected = store.detect().map(|c| apps::join_command(&c[0], &c[1..]));
                self.settings.store_mut(store).command =
                    if Some(text) == detected.as_deref() { String::new() } else { text.to_owned() };
                self.save_settings();
                self.mode = self.settings_at(SettingRow::StoreCommand(store));
            }
            PromptKind::AppStoreUrl => {
                let store = self.settings.app_store_mut();
                store.url = text.to_owned();
                store.name.clear();
                self.save_settings();
                self.store_settings_changed();
                self.mode = self.settings_at(SettingRow::AppStoreUrl);
            }
            PromptKind::ThemeName => {
                if !text.is_empty() {
                    self.themes[self.theme].settings.name = text.to_owned();
                    self.save_theme();
                    let id = self.theme().id.clone();
                    self.select_theme(&id);
                }
                self.mode = self.settings_at(SettingRow::ThemeRename);
            }
            PromptKind::Color(row) if row.page() == Page::GameBox => {
                match if text.is_empty() { Some(None) } else { theme::parse_hex(text).map(Some) } {
                    Some(color) => {
                        if let Some(slot) = self.game_color_slot(row) {
                            *slot = color;
                        }
                        self.save();
                    }
                    None => self.notify(tr!("\"{text}\" is not a color. Use hex, e.g. #0b2a66", text = text)),
                }
                self.mode = self.settings_at(row);
            }
            PromptKind::Color(row @ SettingRow::Button(..)) => {
                match if text.is_empty() { Some(None) } else { theme::parse_hex(text).map(Some) } {
                    Some(color) => {
                        if let Some(slot) = self.button_color_slot(row) {
                            *slot = color;
                        }
                        self.save_theme();
                    }
                    None => self.notify(tr!("\"{text}\" is not a color. Use hex, e.g. #0b2a66", text = text)),
                }
                self.mode = self.settings_at(row);
            }
            PromptKind::CrtSize(row) => {
                let (min, max) = CrtSettings::SIZE_RANGE;
                match text.trim_end_matches("px").trim().replace(',', ".").parse::<f32>() {
                    Ok(px) if px.is_finite() => {
                        if !(min..=max).contains(&px) {
                            self.notify(tr!("Screen sizes go from {min} to {max} px", min = format!("{min:.0}"), max = format!("{max:.0}")));
                        }
                        self.set_crt_size(row, px);
                    }
                    _ => self.notify(tr!("\"{text}\" is not a number of pixels", text = text)),
                }
                self.mode = self.settings_at(row);
            }
            PromptKind::BoxSize(row) => {
                let Some((_, (min, max), _)) = Self::box_size_slot(row) else { return };
                match text.trim_end_matches("mm").trim().replace(',', ".").parse::<f32>() {
                    Ok(mm) if mm.is_finite() => {
                        if !(min..=max).contains(&mm) {
                            self.notify(tr!("Box sizes go from {min} to {max} mm", min = format!("{min:.0}"), max = format!("{max:.0}")));
                        }
                        self.set_box_size(row, mm);
                    }
                    _ => self.notify(tr!("\"{text}\" is not a number of millimetres", text = text)),
                }
                self.mode = self.settings_at(row);
            }
            PromptKind::Color(row) => {
                match theme::parse_hex(text) {
                    Some(color) => {
                        if let Some(slot) = self.color_slot(row) {
                            *slot = color;
                        }
                        self.save_theme();
                    }
                    None => self.notify(tr!("\"{text}\" is not a color. Use hex, e.g. #0b2a66", text = text)),
                }
                self.mode = self.settings_at(row);
            }
            PromptKind::ProfileName => {
                self.mode = self.settings_at(SettingRow::ProfileRename);
                if text == profiles::active() || !self.idle_or_notify() {
                    return;
                }
                match profiles::rename(text) {
                    Ok(()) => self.restart(),
                    Err(e) => self.notify(tr!("Could not rename profile: {error}", error = format!("{e:#}"))),
                }
            }
            PromptKind::ProfileNew => {
                self.mode = self.settings_at(SettingRow::ProfileNew);
                if !self.idle_or_notify() {
                    return;
                }
                match profiles::create(text).and_then(|()| profiles::switch(text)) {
                    Ok(()) => self.restart(),
                    Err(e) => self.notify(tr!("Could not create profile: {error}", error = format!("{e:#}"))),
                }
            }
            PromptKind::Rename => {}
        }
    }

    pub(super) fn cancel_prompt(&mut self) {
        self.mode = match &self.mode {
            Mode::Prompt { kind: PromptKind::AddCommand, .. } => Mode::AddMenu { cursor: add_menu_index(AddSource::Command) },
            Mode::Prompt { kind: PromptKind::StoreCommand(store), .. } => self.settings_at(SettingRow::StoreCommand(*store)),
            Mode::Prompt { kind: PromptKind::ThemeName, .. } => self.settings_at(SettingRow::ThemeRename),
            Mode::Prompt { kind: PromptKind::ProfileName, .. } => self.settings_at(SettingRow::ProfileRename),
            Mode::Prompt { kind: PromptKind::ProfileNew, .. } => self.settings_at(SettingRow::ProfileNew),
            Mode::Prompt { kind: PromptKind::AppStoreUrl, .. } => self.settings_at(SettingRow::AppStoreUrl),
            Mode::Prompt { kind: PromptKind::Color(row) | PromptKind::BoxSize(row) | PromptKind::CrtSize(row), .. } => self.settings_at(*row),
            _ => Mode::Home,
        };
    }

    pub(super) fn draw_options(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let title = self.selected_game().map(|g| g.name.clone()).unwrap_or_default();
        let menu = self.menu();
        let Mode::Options { cursor } = &mut self.mode else { return };
        *cursor = (*cursor).min(menu.len() - 1);
        let rows: Vec<_> = menu.iter().map(|(_, label)| (label.as_str(), None)).collect();
        if let Some(i) = side_panel(ui, screen, s, &title, &rows, cursor) {
            self.run_menu_item(menu[i].0);
        }
    }

    pub(super) fn draw_confirm_remove(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::ConfirmRemove { what, yes } = self.mode else { return };
        let (title, detail) = match what {
            Removal::Game => {
                let Some(game) = self.selected_game() else { return };
                if game.app_id.is_some() {
                    (tr!("Uninstall \"{name}\"?", name = game.name), tr!("Its files are deleted. Its saved data and settings are kept for a reinstall."))
                } else {
                    (tr!("Remove \"{name}\"?", name = game.name), tr!("Only the library entry and its images are removed; game files stay untouched."))
                }
            }
            Removal::Theme => (
                tr!("Delete theme \"{name}\"?", name = self.theme().settings.name),
                tr!("Its folder and images are deleted. Export it first if you want to keep a copy."),
            ),
            Removal::AppStore => (
                tr!("Remove store \"{name}\"?", name = self.settings.app_store().title()),
                tr!("Its address is forgotten. Apps and themes installed from it stay."),
            ),
            Removal::Addon(i) => {
                let Some(addon) = self.addons.installed.get(i) else { return };
                (tr!("Uninstall \"{name}\"?", name = addon.name), tr!("Its files are deleted. Its saved data is kept for a reinstall."))
            }
            Removal::AddonPack(i) => {
                let Some(pack) = crate::appstore::addon_packs().get(i).cloned() else { return };
                (tr!("Delete pack \"{name}\"?", name = pack.file), tr!("The .zip is deleted. An addon installed from it stays installed."))
            }
            Removal::Profile => (
                tr!("Reset profile \"{name}\"?", name = profiles::active()),
                tr!("Its library, settings, themes and apps are deleted. YASAFF restarts."),
            ),
            Removal::Everything => (
                tr!("Reset YASAFF?").to_owned(),
                tr!("Every profile and all data are deleted. YASAFF restarts as on a fresh install."),
            ),
            Removal::StaticOrder => (
                tr!("Mark all games static?").to_owned(),
                tr!("Each stays in its current slot; you place games yourself with Swap position."),
            ),
        };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(160));
        let dialog = Rect::from_center_size(screen.center(), vec2(640.0 * s, 260.0 * s));
        painter.rect_filled(dialog, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        painter.text(dialog.center_top() + vec2(0.0, 50.0 * s), Align2::CENTER_CENTER, title, FontId::proportional(28.0 * s), Color32::WHITE);
        painter.text(dialog.center_top() + vec2(0.0, 95.0 * s), Align2::CENTER_CENTER, detail, FontId::proportional(17.0 * s), Color32::from_gray(170));

        let mut choice = None;
        let action = match what {
            Removal::Theme | Removal::AddonPack(_) => tr!("Delete"),
            Removal::AppStore => tr!("Remove"),
            Removal::Addon(_) => tr!("Uninstall"),
            Removal::Profile | Removal::Everything => tr!("Reset"),
            Removal::StaticOrder => tr!("Turn on"),
            Removal::Game if self.selected_game().is_some_and(|g| g.app_id.is_some()) => tr!("Uninstall"),
            Removal::Game => tr!("Remove"),
        };
        for (i, (label, is_yes)) in [(tr!("Cancel"), false), (action, true)].into_iter().enumerate() {
            let center = dialog.center_bottom() + vec2((i as f32 - 0.5) * 240.0 * s, -60.0 * s);
            let btn = Rect::from_center_size(center, vec2(200.0 * s, 54.0 * s));
            let focused = yes == is_yes;
            painter.rect_filled(btn, (4.0 * s) as u8, if focused { Color32::WHITE } else { Color32::from_white_alpha(25) });
            painter.text(btn.center(), Align2::CENTER_CENTER, label, FontId::proportional(22.0 * s), if focused { Color32::BLACK } else { Color32::WHITE });
            if ui.interact(btn, Id::new(("confirm", i)), Sense::click()).clicked() {
                choice = Some(is_yes);
            }
        }
        if let Some(yes) = choice {
            self.mode = Mode::ConfirmRemove { what, yes };
            self.handle(ui.ctx(), Action::Confirm);
        }
    }

    pub(super) fn draw_prompt(&mut self, ctx: &Context) {
        let gamepad = self.input.has_gamepad();
        let Mode::Prompt { kind, buffer } = &mut self.mode else { return };
        let (title, help) = match kind {
            PromptKind::Rename => (tr!("Rename game"), None),
            PromptKind::AddCommand => (tr!("Add command or URL"), Some(i18n::tr(COMMAND_HELP))),
            PromptKind::EditCommand => (tr!("Edit launch command"), Some(i18n::tr(COMMAND_HELP))),
            PromptKind::StoreCommand(store) => (
                match store {
                    Store::Heroic => tr!("Heroic command"),
                    Store::Steam => tr!("Steam command"),
                },
                Some(tr!("Leave empty to auto-detect. Example: flatpak run com.heroicgameslauncher.hgl")),
            ),
            PromptKind::ThemeName => (tr!("Rename theme"), None),
            PromptKind::ProfileName => (tr!("Rename profile"), Some(tr!("YASAFF restarts after renaming."))),
            PromptKind::ProfileNew => (tr!("New profile name"), Some(tr!("YASAFF restarts into the new, empty profile."))),
            PromptKind::AppStoreUrl => (
                tr!("YASAFF Discover address"),
                Some(tr!("Paste the store's site address, e.g. https://store.example.com. Save it empty to disconnect.")),
            ),
            PromptKind::CrtSize(row) => (
                if *row == SettingRow::CrtWidth { tr!("Screen width (px)") } else { tr!("Screen height (px)") },
                Some(tr!("Pixels at 1080p, from 80 to 600. Scales with the window.")),
            ),
            PromptKind::BoxSize(row) => (
                match row {
                    SettingRow::BoxWidth => tr!("Box width (mm)"),
                    SettingRow::BoxHeight => tr!("Box height (mm)"),
                    _ => tr!("Box depth (mm)"),
                },
                Some(match row {
                    SettingRow::BoxDepth => tr!("Millimetres, from 5 to 100. A DVD case is 14 mm deep."),
                    _ => tr!("Millimetres, from 50 to 400. A DVD case is 135 × 190 mm."),
                }),
            ),
            PromptKind::Color(row) => (
                match row {
                    SettingRow::GradientFrom | SettingRow::Bar(_, BarRow::GradientFrom) => tr!("Gradient start color"),
                    SettingRow::GradientTo | SettingRow::Bar(_, BarRow::GradientTo) => tr!("Gradient end color"),
                    SettingRow::Bar(..) => tr!("Bar color"),
                    SettingRow::Button(_, ButtonRow::TextColor) => tr!("Button letter color"),
                    SettingRow::Button(..) => tr!("Button color"),
                    SettingRow::BoxSpineColor | SettingRow::GameSpineColor => tr!("Spine color"),
                    SettingRow::BoxSpineText | SettingRow::GameSpineText => tr!("Spine text color"),
                    SettingRow::BoxTopColor | SettingRow::GameTopColor => tr!("Top color"),
                    SettingRow::ObjectColor | SettingRow::GameObjectColor => tr!("Object color"),
                    _ => tr!("Background color"),
                },
                Some(if row.page() == Page::GameBox {
                    tr!("Hex color: #rrggbb, e.g. #0b2a66. Leave empty to use the theme's color.")
                } else if matches!(row, SettingRow::Button(..)) {
                    tr!("Hex color: #rrggbb, e.g. #5cb83c. Leave empty for the default. Left/Right in Settings steps through presets.")
                } else if matches!(row, SettingRow::Bar(..)) {
                    tr!("Hex color: #rrggbb, or #rrggbbaa to see through it, e.g. #00000099. Left/Right in Settings steps through presets.")
                } else {
                    tr!("Hex color: #rrggbb, e.g. #0b2a66. Left/Right in Settings steps through presets.")
                }),
            ),
        };
        let mut commit = false;
        let mut cancel = false;
        let modal = egui::Modal::new(Id::new("prompt")).show(ctx, |ui| {
            ui.set_width(if gamepad { 640.0 } else { 560.0 });
            ui.heading(title);
            ui.add_space(8.0);
            let resp = ui.add(egui::TextEdit::singleline(buffer).desired_width(f32::INFINITY));
            if !resp.has_focus() && !resp.lost_focus() {
                resp.request_focus();
            }
            if resp.lost_focus() {
                commit = ui.input(|i| i.key_pressed(egui::Key::Enter));
                cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
            }
            if let Some(help) = help {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(help).small().weak());
            }
            if gamepad {
                ui.add_space(10.0);
                self.keyboard.show(ui, buffer);
            }
            if self.keyboard.take_typed() {
                let end = egui::text::CCursor::new(buffer.chars().count());
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), resp.id) {
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                    state.store(ui.ctx(), resp.id);
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                commit |= ui.button(tr!("Save")).clicked();
                cancel |= ui.button(tr!("Cancel")).clicked();
            });
        });
        if commit {
            self.commit_prompt();
        } else if cancel || modal.should_close() {
            self.cancel_prompt();
        }
    }

    pub(super) fn remove_selected(&mut self) {
        if self.selected < self.library.games.len() {
            self.library.remove(self.selected);
            self.selected = self.selected.min(self.library.games.len());
            self.save();
        }
    }

    pub(super) fn delete_theme(&mut self) {
        if !theme::deletable(&self.themes, self.theme) {
            return;
        }
        if let Err(e) = self.theme().delete() {
            self.notify(tr!("Could not delete theme: {error}", error = format!("{e:#}")));
            return;
        }
        let removed = self.themes.remove(self.theme);
        let id = self.themes[self.theme.min(self.themes.len() - 1)].id.clone();
        self.select_theme(&id);
        self.notify(tr!("Deleted {name}", name = removed.settings.name));
    }

    pub(super) fn draw_toast(&mut self, painter: &Painter, screen: Rect, s: f32) {
        let Some((msg, at)) = &self.toast else { return };
        let age = at.elapsed();
        if age > TOAST_TIME {
            self.toast = None;
            return;
        }
        let alpha = ((TOAST_TIME - age).as_secs_f32() / 0.4).min(1.0);
        let font = FontId::proportional(20.0 * s);
        let galley = painter.layout(msg.clone(), font, Color32::WHITE.gamma_multiply(alpha), screen.width() * 0.6);
        let rect = Rect::from_center_size(
            pos2(screen.center().x, screen.bottom() - 120.0 * s),
            galley.size() + vec2(40.0 * s, 24.0 * s),
        );
        painter.rect_filled(rect, (6.0 * s) as u8, Color32::from_black_alpha((220.0 * alpha) as u8));
        painter.galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
    }
}

impl App {
    pub(super) fn handle_options(&mut self, action: Action) {
        let Mode::Options { cursor } = &mut self.mode else { return };
        match action {
            Action::Up => *cursor = cursor.saturating_sub(1),
            Action::Down => *cursor += 1,
            Action::Confirm => {
                let i = *cursor;
                if let Some(&(item, _)) = self.menu().get(i) {
                    self.run_menu_item(item);
                }
            }
            Action::Back | Action::Options => self.mode = Mode::Home,
            _ => {}
        }
    }

    pub(super) fn handle_confirm_remove(&mut self, action: Action) {
        let Mode::ConfirmRemove { what, yes } = &mut self.mode else { return };
        match action {
            Action::Left | Action::Right | Action::Up | Action::Down => *yes = !*yes,
            Action::Confirm => {
                let (what, yes) = (*what, *yes);
                match what {
                    Removal::Game => {
                        self.mode = Mode::Home;
                        if yes && self.selected_game().is_some_and(|g| g.app_id.is_some()) {
                            self.uninstall_selected();
                        } else if yes {
                            self.remove_selected();
                        }
                    }
                    Removal::Theme => {
                        self.mode = self.settings_at(SettingRow::Theme);
                        if yes {
                            self.delete_theme();
                        }
                    }
                    Removal::AppStore => {
                        if yes {
                            let title = self.settings.app_store().title();
                            self.settings.remove_app_store();
                            self.save_settings();
                            self.store_settings_changed();
                            self.notify(tr!("Removed {name}", name = title));
                        }
                        self.mode = self.settings_at(SettingRow::AppStoreActive);
                    }
                    Removal::Addon(i) => {
                        if yes {
                            self.uninstall_addon(i);
                        }
                        self.mode = self.addon_manager_after_uninstall(i, yes);
                    }
                    Removal::AddonPack(i) => self.delete_addon_pack(i, yes),
                    Removal::StaticOrder => {
                        self.mode = self.settings_at(SettingRow::OrderStatic);
                        if yes {
                            self.settings.order.static_order = true;
                            self.save_settings();
                        }
                    }
                    Removal::Profile | Removal::Everything => {
                        let row = if what == Removal::Profile { SettingRow::ProfileReset } else { SettingRow::AppReset };
                        self.mode = self.settings_at(row);
                        if yes {
                            self.reset_profiles(what == Removal::Everything);
                        }
                    }
                }
            }
            Action::Back => {
                let what = *what;
                self.mode = match what {
                    Removal::Game => Mode::Options { cursor: self.menu_index(MenuItem::Remove) },
                    Removal::Theme => self.settings_at(SettingRow::ThemeDelete),
                    Removal::AppStore => self.settings_at(SettingRow::AppStoreRemove),
                    Removal::Addon(i) => self.addon_manager_after_uninstall(i, false),
                    Removal::AddonPack(i) => return self.delete_addon_pack(i, false),
                    Removal::Profile => self.settings_at(SettingRow::ProfileReset),
                    Removal::Everything => self.settings_at(SettingRow::AppReset),
                    Removal::StaticOrder => self.settings_at(SettingRow::OrderStatic),
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_prompt(&mut self, action: Action) {
        let Mode::Prompt { buffer, .. } = &mut self.mode else { return };
        match action {
            Action::Up | Action::Down | Action::Left | Action::Right => self.keyboard.navigate(action),
            Action::Confirm => self.keyboard.press_selected(buffer),
            Action::Erase => self.keyboard.erase(buffer),
            Action::Add => self.keyboard.toggle_shift(),
            Action::Options => self.commit_prompt(),
            Action::Back => self.cancel_prompt(),
            _ => {}
        }
    }
}
