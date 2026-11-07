//! Which rows each Settings page has, and the sections and headings they are grouped under.

use super::*;

impl App {
    pub(in crate::app) fn setting_rows(&self, page: Page) -> Vec<SettingRow> {
        use SettingRow as R;
        if page == Page::GameDetails {
            if self.library.categories.is_empty() {
                return vec![R::GameNoCategories];
            }
            let mut rows = vec![R::GameMainCategory];
            rows.extend((0..self.library.categories.len()).map(R::GameCategory));
            return rows;
        }
        if page == Page::ThemePictures {
            let mut rows = Vec::new();
            for slot in self.picture_slots() {
                rows.push(R::ThemePicture(slot));
                if self.picture_key(slot).is_some_and(|key| self.theme().picture(&key).is_some()) {
                    rows.extend([R::ThemePictureAdjust(slot), R::ThemePictureClear(slot)]);
                }
            }
            if rows.is_empty() {
                rows.push(R::ThemePicturesEmpty);
            }
            return rows;
        }
        if page == Page::GameDevice {
            let mut rows = vec![R::DeviceTarget, R::DeviceCreateFile, R::DeviceBurn, R::DeviceInsertConfig];
            let Some(game) = self.selected_game().filter(|g| g.app_id.is_none()) else { return rows };
            rows.push(R::DeviceCreateCatalog);
            if game.save_dir.is_some() {
                rows.push(R::DeviceBackupNow);
            }
            rows.extend([R::DeviceKeepSaves, R::DeviceRecovery]);
            if game.save_dir.is_some() {
                rows.push(R::DeviceOpenSaveFolder);
            }
            rows.push(R::DeviceSetSaveFolder);
            rows.extend((0..game.device_saves.paths.len()).map(R::DeviceSavePath));
            if game.device_saves.paths.is_empty() && game.save_dir.is_some() {
                rows.push(R::DeviceSaveFolder);
            }
            rows.extend([R::DeviceAddFiles, R::DeviceAddFolders]);
            return rows;
        }
        if page == Page::GameBox {
            let game = self.selected_game();
            let has = |kind: AssetKind| game.is_some_and(|g| g.asset(kind).is_some());
            // Each way of showing the game is a group of sections; the one shown now goes first.
            let mut modes: Vec<(bool, Vec<SettingRow>)> = Vec::new();
            for style in TileStyle::ALL {
                let mut rows = Vec::new();
                if style.shows_cover() {
                    rows.push(R::StyleCover(style));
                    if let Some(game) = game.filter(|g| g.style_cover(style).is_some()) {
                        rows.push(R::StyleAdjust(style));
                        if !game.style_placement(style).is_default() {
                            rows.push(R::StyleAdjustReset(style));
                        }
                    }
                }
                if style == TileStyle::Case {
                    // The box sides are shared by every box style.
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
                    rows.extend([R::GameSpineLabel, R::GameSpineColor, R::GameSpineText, R::GameTopColor]);
                }
                if style == TileStyle::Object {
                    rows.push(R::GameModel);
                    if has(AssetKind::Model) {
                        rows.extend([R::GameModelUp, R::GameModelClear]);
                    }
                    rows.push(R::GameObjectColor);
                }
                modes.push((self.shown_style() == Some(style), rows));
            }
            if self.theme_manifest().is_some_and(|m| m.game_picture.is_some()) {
                let mut rows = vec![R::GameThemePicture];
                if let Some(game) = game.filter(|g| g.asset(AssetKind::ThemePicture).is_some()) {
                    rows.push(R::GameAdjust(AssetKind::ThemePicture));
                    if !game.placement(AssetKind::ThemePicture).is_default() {
                        rows.push(R::GameAdjustReset(AssetKind::ThemePicture));
                    }
                    rows.push(R::GameThemePictureClear);
                }
                modes.push((true, rows));
            }
            modes.sort_by_key(|(shown, _)| !shown);
            let mut rows: Vec<SettingRow> = modes.into_iter().flat_map(|(_, rows)| rows).collect();
            rows.push(R::GameBackground);
            if has(AssetKind::Background) {
                rows.push(R::GameBackgroundClear);
            }
            rows.push(R::GameVideo);
            if has(AssetKind::Video) {
                rows.push(R::GameVideoClear);
            }
            rows.push(R::GameIdleVideo);
            if has(AssetKind::IdleVideo) {
                rows.push(R::GameIdleVideoClear);
            }
            rows.push(R::GameTitleFont);
            match game.map_or(0, |g| g.gallery.len()) {
                0 => rows.push(R::GalleryEmpty),
                n => rows.extend((0..n).map(R::GalleryImage)),
            }
            return rows;
        }
        let theme = &self.theme().settings;
        let mut rows = vec![R::Fullscreen, R::Language, R::ShowLastPlayed, R::GalleryKeepAll, R::Autostart];
        if cfg!(windows) {
            rows.push(R::TaskbarHide);
        }
        rows.extend([R::OrderLastPlayed, R::OrderStatic]);
        rows.extend([R::ProfileActive, R::ProfileRename, R::ProfileNew, R::ProfileExport, R::ProfileImport, R::ProfileReset]);
        rows.push(R::CategoryAdd);
        rows.extend((0..self.library.categories.len()).map(R::Category));
        rows.extend([R::ViewKind, R::TileStyle]);
        if theme.kind == ThemeKind::Bubbles {
            rows.extend([R::BubbleSize, R::BubbleVariance, R::BubbleFill, R::BubbleShuffle, R::BubbleVideo, R::BubbleSound]);
            if theme.bubbles.sound {
                rows.push(R::BubbleCustomSound);
                if theme.bubbles.custom_sound {
                    rows.push(R::BubbleSoundFile);
                }
            }
        } else {
            match theme.view {
            _ if theme.kind == ThemeKind::Custom => {
                rows.extend((0..self.theme_manifest().map_or(0, |m| m.options.len())).map(R::ThemeOption));
            }
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
        let tiles_used = theme.kind == ThemeKind::Standard;
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
            if store == Store::Steam {
                rows.extend([R::SteamShowOwned, R::SteamSource]);
                match self.settings.steam_source {
                    SteamSource::SignIn => rows.push(R::SteamSignIn),
                    SteamSource::ApiKey => rows.extend([R::SteamApiKey, R::SteamAccount]),
                    SteamSource::LocalFiles => rows.push(R::SteamAccount),
                }
            }
        }
        rows.extend([R::DiscOnInsert, R::DiscScanAtStart, R::DiscRemovable, R::DiscHideFileWindow, R::DiscKeepSaves]);
        rows.push(R::AddonManager);
        rows.extend([R::AboutApp, R::AboutSource, R::AboutAuthor, R::AboutServer, R::AppReset]);
        rows
    }

    pub(in crate::app) fn setting_enabled(&self, row: SettingRow) -> bool {
        match row {
            SettingRow::TileBar => self.theme().settings.tiles.rows <= 1,
            SettingRow::ViewKind | SettingRow::TileStyle => self.is_standard(),
            SettingRow::DiscAngle => !self.theme().settings.discs.lying_flat(),
            SettingRow::ThemeOption(i) => self.theme_option_enabled(i),
            SettingRow::ThemePicture(_) | SettingRow::ThemePictureAdjust(_) | SettingRow::ThemePictureClear(_) => true,
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
            SettingRow::Autostart => Some((tr!("System"), None)),
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
            SettingRow::Theme if !self.is_standard() => {
                Some((tr!("Theme"), Some((tr!("Custom theme · Not all options are available in this pre-defined theme").to_owned(), PAD_Y))))
            }
            SettingRow::Theme => Some((tr!("Theme"), Some((tr!("{count} installed", count = self.themes.len()), Color32::from_gray(140))))),
            SettingRow::ViewKind => Some((tr!("Home screen"), None)),
            SettingRow::CategoryAdd => {
                let n = self.library.categories.len();
                let status = tr_n!(n, "{n} category · Games pick theirs in Options › Details", "{n} categories · Games pick theirs in Options › Details");
                Some((tr!("Categories"), Some((status, Color32::from_gray(140)))))
            }
            SettingRow::ThemeOption(i) => self.theme_option_heading(i),
            SettingRow::ThemePicture(PictureSlot::Category(0)) => Some((tr!("Your categories"), None)),
            SettingRow::ThemePicture(PictureSlot::Slot(i)) => {
                let slot = self.picture_spec(i)?;
                let section = slot.section.as_deref().map(intern).or((i == 0).then(|| tr!("More pictures")))?;
                Some((section, slot.section_description.as_ref().map(|d| (d.clone(), Color32::from_gray(140)))))
            }
            SettingRow::GameMainCategory | SettingRow::GameNoCategories => Some((
                tr!("Categories"),
                Some((tr!("The game shows in each of its categories. The main one is the first it joined, unless you pick another").to_owned(), Color32::from_gray(140))),
            )),
            SettingRow::GameThemePicture => {
                let picture = self.theme_manifest()?.game_picture.as_ref()?;
                let status = (!picture.section_description.is_empty()).then(|| (picture.section_description.clone(), Color32::from_gray(140)));
                Some((intern(&picture.section), status))
            }
            SettingRow::BoxCase => {
                Some((tr!("Boxes"), Some((tr!("Defaults. Each game can change them in its Options").to_owned(), Color32::from_gray(140)))))
            }
            SettingRow::StyleCover(style) => Some((style.label(), Some(self.style_status(style)))),
            SettingRow::GameSpine => Some((
                tr!("Box sides"),
                Some((tr!("Shared by every box style. Colors and the label are used where there is no image").to_owned(), Color32::from_gray(140))),
            )),
            SettingRow::GameBackground => Some((tr!("Background"), None)),
            SettingRow::GalleryImage(0) | SettingRow::GalleryEmpty => Some(self.gallery_heading()),
            SettingRow::GameArt => Some((tr!("Background"), None)),
            SettingRow::Bar(slot, BarRow::Enabled) => Some((slot.label(), None)),
            SettingRow::ButtonStyle => Some(self.buttons_heading()),
            SettingRow::TileIcon(special) if special == Special::ALL[0] => Some((
                tr!("Tile icons"),
                Some((tr!("SVG icons for the Discover, Add game and Settings tiles. Saved and exported with the theme").to_owned(), Color32::from_gray(140))),
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
            SettingRow::GameVideo => Some((tr!("Videos"), Some((tr!("For bubbles and CRT screens").to_owned(), Color32::from_gray(140))))),
            SettingRow::CrtWidth => Some((tr!("CRT screens"), Some((tr!("Videos are set in each game's Options").to_owned(), Color32::from_gray(140))))),
            SettingRow::GameModel => Some((
                TileStyle::Object.label(),
                Some((tr!("Shows the 3D model, or the name in 3D letters without one").to_owned(), Color32::from_gray(140))),
            )),
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
                tr!("Device"),
                Some(match self.devices.len() {
                    0 => (tr!("Plug in a USB stick or SD card. It shows up here").to_owned(), PAD_Y),
                    n => (tr_n!(n, "{n} USB stick or SD card plugged in", "{n} USB sticks or SD cards plugged in"), Color32::from_gray(140)),
                }),
            )),
            SettingRow::DeviceCreateFile => Some((
                tr!("YASAFF Launching File"),
                Some((tr!("A .yasaff file on a disc, USB stick or SD card starts this game").to_owned(), Color32::from_gray(140))),
            )),
            SettingRow::DeviceCreateCatalog => Some((
                tr!("Backups"),
                Some(if self.settings.discs.keep_saves {
                    (tr!("Saves are copied when the game closes, if it was started from a USB stick or SD card").to_owned(), Color32::from_gray(140))
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

    /// The tile style games are shown in now; none in a custom theme, which draws them its own way.
    fn shown_style(&self) -> Option<TileStyle> {
        (!self.is_custom()).then_some(self.theme().settings.tile_style)
    }

    /// Whether `row` heads the section of the way games are shown now.
    fn setting_shown(&self, row: SettingRow) -> bool {
        match row {
            SettingRow::StyleCover(style) => self.shown_style() == Some(style),
            SettingRow::GameModel => self.shown_style() == Some(TileStyle::Object),
            SettingRow::GameThemePicture => true,
            _ => false,
        }
    }

    fn style_status(&self, style: TileStyle) -> (String, Color32) {
        let gray = Color32::from_gray(140);
        let Some(game) = self.selected_game() else { return (String::new(), gray) };
        if self.shown_style() == Some(style) {
            (tr!("The tile style shown now").to_owned(), ACCENT)
        } else if game.style_cover(style).is_some() {
            (tr!("Has its own cover and placement").to_owned(), gray)
        } else {
            (tr!("No cover of its own. It copies the one shown before it when you switch to it").to_owned(), gray)
        }
    }

    fn gallery_heading(&self) -> (&'static str, Option<(String, Color32)>) {
        let n = self.selected_game().map_or(0, |g| g.gallery.len());
        let status = if self.settings.gallery.keep_all {
            tr_n!(n, "{n} image · replaced images stay here until you remove them", "{n} images · replaced images stay here until you remove them")
        } else {
            tr_n!(n, "{n} image · replaced images are deleted when nothing uses them", "{n} images · replaced images are deleted when nothing uses them")
        };
        (tr!("Gallery"), Some((status, Color32::from_gray(140))))
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
                    sections.push(SettingSection { title, status, shown: self.setting_shown(row), rows: i..i + 1 });
                }
            }
        }
        sections
    }
}
