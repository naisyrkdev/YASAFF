//! Which rows each Settings page has, and the sections and headings they are grouped under.

use super::*;

impl App {
    pub(in crate::app) fn setting_rows(&self, page: Page) -> Vec<SettingRow> {
        use SettingRow as R;
        if page == Page::GameDevice {
            let mut rows = vec![R::DeviceTarget, R::DeviceInsertConfig, R::DeviceCreateCatalog, R::DeviceKeepSaves, R::DeviceRecovery];
            if let Some(game) = self.selected_game() {
                rows.extend((0..game.device_saves.paths.len()).map(R::DeviceSavePath));
                if game.device_saves.paths.is_empty() && game.save_dir.is_some() {
                    rows.push(R::DeviceSaveFolder);
                }
            }
            rows.extend([R::DeviceAddFiles, R::DeviceAddFolders]);
            return rows;
        }
        if page == Page::GameBox {
            let game = self.selected_game();
            let mut rows = vec![R::GameAdjust(AssetKind::Cover)];
            if game.is_some_and(|g| !g.placement(AssetKind::Cover).is_default()) {
                rows.push(R::GameAdjustReset(AssetKind::Cover));
            }
            rows.push(R::GameCover);
            for (kind, pick, clear) in [
                (AssetKind::Spine, R::GameSpine, R::GameSpineClear),
                (AssetKind::Top, R::GameTop, R::GameTopClear),
                (AssetKind::Back, R::GameBack, R::GameBackClear),
                (AssetKind::Side, R::GameSide, R::GameSideClear),
                (AssetKind::Bottom, R::GameBottom, R::GameBottomClear),
                (AssetKind::Booklet, R::GameBooklet, R::GameBookletClear),
            ] {
                rows.push(pick);
                let Some(game) = game.filter(|g| g.asset(kind).is_some()) else { continue };
                rows.push(R::GameAdjust(kind));
                if !game.placement(kind).is_default() {
                    rows.push(R::GameAdjustReset(kind));
                }
                rows.push(clear);
            }
            rows.push(R::GameModel);
            if game.is_some_and(|g| g.asset(AssetKind::Model).is_some()) {
                rows.extend([R::GameModelUp, R::GameModelClear]);
            }
            rows.push(R::GameVideo);
            if game.is_some_and(|g| g.asset(AssetKind::Video).is_some()) {
                rows.push(R::GameVideoClear);
            }
            rows.push(R::GameIdleVideo);
            if game.is_some_and(|g| g.asset(AssetKind::IdleVideo).is_some()) {
                rows.push(R::GameIdleVideoClear);
            }
            rows.push(R::GameObjectColor);
            rows.extend([R::GameSpineLabel, R::GameSpineColor, R::GameSpineText, R::GameTopColor, R::GameTitleFont]);
            return rows;
        }
        let theme = &self.theme().settings;
        let mut rows = vec![R::Fullscreen, R::Autostart, R::Language, R::OrderLastPlayed, R::OrderStatic];
        rows.extend([R::ProfileActive, R::ProfileRename, R::ProfileNew, R::ProfileExport, R::ProfileImport, R::ProfileReset]);
        rows.extend([R::ViewKind, R::TileStyle]);
        let bubbles = theme.kind == ThemeKind::Bubbles;
        if bubbles {
            rows.extend([R::BubbleSize, R::BubbleVariance, R::BubbleFill, R::BubbleShuffle, R::BubbleVideo, R::BubbleSound]);
            if theme.bubbles.sound {
                rows.push(R::BubbleCustomSound);
                if theme.bubbles.custom_sound {
                    rows.push(R::BubbleSoundFile);
                }
            }
        } else {
            match theme.view {
            ViewKind::Row => {
                rows.push(R::TileRows);
                if theme.tile_style != TileStyle::Crt {
                    rows.push(R::TileSize);
                }
                rows.extend([R::TileSpacing, R::EnlargeSelected]);
                if theme.tiles.enlarge_selected {
                    rows.push(R::SelectedScale);
                }
                rows.push(R::TileBar);
            }
            ViewKind::Grid => rows.extend([R::GridColumns, R::GridRows, R::GridSpacing, R::GridEnlarge, R::GridAlign]),
            }
            rows.push(R::SwapSound);
            if theme.swap.sound {
                rows.push(R::SwapCustomSound);
                if theme.swap.custom_sound {
                    rows.push(R::SwapSoundFile);
                }
            }
        }
        rows.extend([R::GameArt, R::BackgroundKind]);
        rows.extend_from_slice(match theme.background.kind {
            BackgroundKind::Color => &[R::BackgroundColor][..],
            BackgroundKind::Gradient => &[R::GradientFrom, R::GradientTo, R::GradientDirection],
            BackgroundKind::Image => &[R::BackgroundImage, R::ImageFit, R::BackgroundColor],
        });
        for slot in BarSlot::ALL {
            let bar = theme.bar(slot);
            rows.push(R::Bar(slot, BarRow::Enabled));
            if !bar.enabled {
                continue;
            }
            rows.extend([R::Bar(slot, BarRow::Height), R::Bar(slot, BarRow::Kind)]);
            let kind_rows: &[BarRow] = match bar.kind {
                BackgroundKind::Color => &[BarRow::Color],
                BackgroundKind::Gradient => &[BarRow::GradientFrom, BarRow::GradientTo, BarRow::GradientDirection],
                BackgroundKind::Image if self.theme().bar_image(slot).is_none() => &[BarRow::Image, BarRow::Color],
                BackgroundKind::Image if bar.image_placed_by_default() => {
                    &[BarRow::Image, BarRow::ImageZoom, BarRow::ImageX, BarRow::ImageY, BarRow::ImageRotation, BarRow::Color]
                }
                BackgroundKind::Image => &[
                    BarRow::Image,
                    BarRow::ImageZoom,
                    BarRow::ImageX,
                    BarRow::ImageY,
                    BarRow::ImageRotation,
                    BarRow::ImageReset,
                    BarRow::Color,
                ],
            };
            rows.extend(kind_rows.iter().map(|&r| R::Bar(slot, r)));
        }
        rows.extend([R::ButtonStyle, R::ButtonPick]);
        let button = self.button_edit;
        let button_rows: &[ButtonRow] = match (self.theme().button_icon(button), theme.buttons.style()) {
            (Some(_), _) => &[ButtonRow::Icon, ButtonRow::IconClear],
            (None, ButtonStyle::Images) => &[ButtonRow::Icon],
            (None, ButtonStyle::Drawn) => &[ButtonRow::Icon, ButtonRow::Shape, ButtonRow::Color, ButtonRow::TextColor],
        };
        rows.extend(button_rows.iter().map(|&r| R::Button(button, r)));
        for special in Special::ALL {
            rows.push(R::TileIcon(special));
            if self.theme().tile_icon(special.key()).is_some() {
                rows.push(R::TileIconClear(special));
            }
        }
        rows.extend([R::Theme, R::ThemeRename, R::ThemeNew, R::ThemeExport, R::ThemeImport]);
        if theme::deletable(&self.themes, self.theme) {
            rows.push(R::ThemeDelete);
        }
        let tiles_used = !bubbles;
        if tiles_used && (theme.tile_style.is_box() || theme.tile_style == TileStyle::Cover) {
            rows.push(R::BoxCase);
            if theme.boxes.case == BoxCase::Custom {
                rows.extend([R::BoxWidth, R::BoxHeight]);
                if theme.tile_style.is_box() {
                    rows.push(R::BoxDepth);
                }
            }
        }
        if tiles_used && theme.tile_style.is_box() {
            rows.extend([R::BoxSpineLabel, R::BoxSpineColor, R::BoxSpineText, R::BoxTopColor]);
            if theme.tile_style == TileStyle::Case {
                rows.push(R::DiscKind);
            }
        } else if tiles_used && theme.tile_style == TileStyle::Object {
            rows.push(R::ObjectColor);
        } else if tiles_used && theme.tile_style == TileStyle::Disc {
            rows.extend([R::DiscKind, R::DiscAngle, R::DiscSpin]);
            if theme.discs.spin {
                rows.extend([R::DiscSpinDirection, R::DiscSpinSpeed]);
            }
        } else if tiles_used && theme.tile_style == TileStyle::Crt {
            rows.extend([R::CrtWidth, R::CrtHeight, R::CrtVideo, R::CrtIdleVideo, R::CrtFilter]);
        }
        rows.extend([R::AppStoreActive, R::AppStoreUrl, R::AppStoreTest, R::AppStoreAdd]);
        if self.settings.app_stores.len() > 1 || !self.settings.app_store().is_empty() {
            rows.push(R::AppStoreRemove);
        }
        rows.push(R::VirtualMouse);
        if self.settings.mouse.virtual_mouse && !self.global_mouse.connected() {
            rows.push(R::MouseSpeed);
        }
        rows.push(R::MouseCursor);
        if !self.settings.mouse.cursor.is_empty() {
            rows.push(R::MouseCursorClear);
        }
        rows.push(R::MouseCursorSize);
        rows.push(R::ControllerPrompts);
        let custom = self.settings.controls.is_custom();
        if !custom {
            rows.push(R::ControllerSwap);
        }
        rows.extend(PadButton::ALL.map(R::ControllerButton));
        if custom {
            rows.push(R::ControllerReset);
        }
        rows.extend(Special::PLACEABLE.map(R::SpecialPlace));
        rows.extend([R::FontTitle, R::FontImport, R::FontManage]);
        for store in Store::ALL {
            rows.extend([R::StoreEnabled(store), R::StoreBackground(store), R::StoreCommand(store)]);
        }
        rows.extend([R::DiscOnInsert, R::DiscScanAtStart, R::DiscRemovable, R::DiscKeepSaves]);
        rows.push(R::AddonManager);
        rows.extend([R::AboutApp, R::AboutSource, R::AboutServer, R::AppReset]);
        rows
    }

    pub(in crate::app) fn setting_enabled(&self, row: SettingRow) -> bool {
        match row {
            SettingRow::TileBar => self.theme().settings.tiles.rows <= 1,
            SettingRow::ViewKind | SettingRow::TileStyle => !self.is_bubbles(),
            SettingRow::DiscAngle => !self.theme().settings.discs.lying_flat(),
            SettingRow::OrderLastPlayed => !self.settings.order.static_order,
            SettingRow::SpecialPlace(special) => self.special_button(special).is_some(),
            _ => true,
        }
    }

    pub(in crate::app) fn settings_cursor(&self, row: SettingRow) -> usize {
        self.setting_rows(row.page()).iter().position(|r| *r == row).unwrap_or(0)
    }

    pub(in crate::app) fn settings_at(&self, row: SettingRow) -> Mode {
        Mode::Settings { cursor: self.settings_cursor(row), page: row.page(), in_rows: true }
    }

    pub(in crate::app) fn setting_heading(&self, row: SettingRow) -> Option<(&'static str, Option<(String, Color32)>)> {
        match row {
            SettingRow::Fullscreen => Some((tr!("General"), None)),
            SettingRow::OrderLastPlayed => Some((
                tr!("Game order"),
                Some((tr!("Row and grid views · Options on a game swaps it or marks its position as static").to_owned(), Color32::from_gray(140))),
            )),
            SettingRow::ProfileActive => Some((
                tr!("Profiles"),
                Some((tr!("Profile: {name}", name = profiles::active()), Color32::from_gray(140))),
            )),
            SettingRow::AppReset => Some((tr!("Power options"), None)),
            SettingRow::AppStoreActive => Some((tr!("YASAFF Discover"), Some(self.store_status()))),
            SettingRow::AboutApp => Some((tr!("About"), None)),
            SettingRow::AddonManager => Some(self.addons_heading()),
            SettingRow::VirtualMouse => Some((tr!("Mouse"), self.global_mouse_status())),
            SettingRow::ControllerPrompts => Some((tr!("Controller"), Some(self.controller_warning()))),
            SettingRow::SpecialPlace(Special::Store) => Some((
                tr!("Discover and Add game"),
                Some((tr!("Assign each a controller button above, then place it in the top or bottom bar").to_owned(), Color32::from_gray(140))),
            )),
            SettingRow::FontTitle => Some((tr!("Fonts"), Some((tr!("{count} imported", count = self.fonts.len()), Color32::from_gray(140))))),
            SettingRow::Theme if self.is_bubbles() => {
                Some((tr!("Theme"), Some((tr!("Custom theme · Not all options are available in this pre-defined theme").to_owned(), PAD_Y))))
            }
            SettingRow::Theme => Some((tr!("Theme"), Some((tr!("{count} installed", count = self.themes.len()), Color32::from_gray(140))))),
            SettingRow::ViewKind => Some((tr!("Home screen"), None)),
            SettingRow::BoxCase => {
                Some((tr!("Boxes"), Some((tr!("Defaults; each game can change them in its Options").to_owned(), Color32::from_gray(140)))))
            }
            SettingRow::GameAdjust(AssetKind::Cover) => {
                Some((tr!("Cover in its tile"), Some((tr!("Squares, game covers and box fronts").to_owned(), Color32::from_gray(140)))))
            }
            SettingRow::GameCover => Some((tr!("Images"), None)),
            SettingRow::GameSpineLabel => {
                Some((tr!("Spine and top"), Some((tr!("Used where there is no spine / top image").to_owned(), Color32::from_gray(140)))))
            }
            SettingRow::GameArt => Some((tr!("Background"), None)),
            SettingRow::Bar(slot, BarRow::Enabled) => Some((slot.label(), None)),
            SettingRow::ButtonStyle => Some(self.buttons_heading()),
            SettingRow::TileIcon(special) if special == Special::ALL[0] => Some((
                tr!("Tile icons"),
                Some((tr!("SVG icons for the Discover, Add game and Settings tiles; saved and exported with the theme").to_owned(), Color32::from_gray(140))),
            )),
            SettingRow::DiscKind => Some((tr!("Discs"), Some((tr!("Covers are printed on the label side").to_owned(), Color32::from_gray(140))))),
            SettingRow::ObjectColor => Some((
                tr!("3D objects"),
                Some((tr!("Games without a 3D model show their name in 3D letters").to_owned(), Color32::from_gray(140))),
            )),
            SettingRow::BubbleSize => {
                let per_page = self.bubble_layout.as_ref().map(|l| (l.per_page, l.pages()));
                let sub = per_page.map(|(n, pages)| {
                    let fits = tr_n!(n, "Up to {n} bubble per page", "Up to {n} bubbles per page");
                    let text = if pages > 1 { tr!("{fits} · {pages} pages", fits = fits, pages = pages) } else { fits };
                    (text, Color32::from_gray(140))
                });
                Some((tr!("Bubbles"), sub))
            }
            SettingRow::GameVideo => Some((tr!("Videos"), Some((tr!("For bubbles and CRT screens; needs ffmpeg installed").to_owned(), Color32::from_gray(140))))),
            SettingRow::CrtWidth => Some((tr!("CRT screens"), Some((tr!("Videos are set in each game's Options").to_owned(), Color32::from_gray(140))))),
            SettingRow::GameModel => Some((tr!("3D object"), Some((tr!("For the 3D objects tile style").to_owned(), Color32::from_gray(140))))),
            SettingRow::GameTitleFont => Some((tr!("Title"), None)),
            SettingRow::StoreEnabled(store) => {
                let status = match self.settings.store_command(store) {
                    Some(c) => (tr!("Found: {command}", command = apps::join_command(&c[0], &c[1..])), Color32::from_gray(140)),
                    None => (tr!("Not found").to_owned(), PAD_B),
                };
                Some((store.name(), Some(status)))
            }
            SettingRow::DiscOnInsert => {
                Some((tr!("Physical Devices"), Some((tr!("A disc or USB stick with a .yasaff file").to_owned(), Color32::from_gray(140)))))
            }
            SettingRow::DeviceTarget => Some((
                tr!("Launch device"),
                Some(match self.devices.len() {
                    0 => (tr!("Plug in a USB stick or SD card; it shows up here").to_owned(), PAD_Y),
                    n => (tr_n!(n, "{n} USB stick or SD card plugged in", "{n} USB sticks or SD cards plugged in"), Color32::from_gray(140)),
                }),
            )),
            SettingRow::DeviceKeepSaves => Some((
                tr!("Saves on device"),
                Some(if self.settings.discs.keep_saves {
                    (tr!("Copied when the game closes, if it was started from a USB stick or SD card").to_owned(), Color32::from_gray(140))
                } else {
                    (tr!("Also turn on \"Keep saves on the launch device\" in Settings › Physical Devices").to_owned(), PAD_Y)
                }),
            )),
            SettingRow::DeviceSavePath(0) | SettingRow::DeviceSaveFolder => Some(self.device_files_heading()),
            SettingRow::DeviceAddFiles if self.selected_game().is_none_or(|g| crate::app::device_save_paths(g).is_empty()) => {
                Some(self.device_files_heading())
            }
            _ => None,
        }
    }

    pub(in crate::app) fn device_files_heading(&self) -> (&'static str, Option<(String, Color32)>) {
        let status = match self.selected_game().map(|g| g.device_saves.paths.len()) {
            Some(0) | None => (tr!("Pick the files or folders the game saves to").to_owned(), Color32::from_gray(140)),
            Some(n) => (tr_n!(n, "{n} tracked", "{n} tracked"), Color32::from_gray(140)),
        };
        (tr!("Save files"), Some(status))
    }

    pub(in crate::app) fn addons_heading(&self) -> (&'static str, Option<(String, Color32)>) {
        (tr!("Addons"), Some((tr!("{count} installed", count = self.addons.installed.len()), Color32::from_gray(140))))
    }

    pub(in crate::app) fn buttons_heading(&self) -> (&'static str, Option<(String, Color32)>) {
        (tr!("Buttons"), Some((tr!("How controller buttons look in the button hints").to_owned(), Color32::from_gray(140))))
    }

    pub(in crate::app) fn setting_sections(&self, rows: &[SettingRow]) -> Vec<SettingSection> {
        let mut sections: Vec<SettingSection> = Vec::new();
        for (i, &row) in rows.iter().enumerate() {
            match (self.setting_heading(row), sections.last_mut()) {
                (None, Some(last)) => last.rows.end = i + 1,
                (heading, _) => {
                    let (title, status) = heading.unwrap_or((tr!("General"), None));
                    sections.push(SettingSection { title, status, rows: i..i + 1 });
                }
            }
        }
        sections
    }
}
