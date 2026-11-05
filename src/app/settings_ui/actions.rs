//! What Confirm and Left/Right do on a Settings row.

use super::*;

impl App {
    pub(in crate::app) fn color_slot(&mut self, row: SettingRow) -> Option<&mut Color32> {
        let settings = &mut self.themes[self.theme].settings;
        if let SettingRow::Bar(slot, field) = row {
            let bar = settings.bar_mut(slot);
            return match field {
                BarRow::Color => Some(&mut bar.color),
                BarRow::GradientFrom => Some(&mut bar.gradient_from),
                BarRow::GradientTo => Some(&mut bar.gradient_to),
                _ => None,
            };
        }
        let (bg, boxes, objects) = (&mut settings.background, &mut settings.boxes, &mut settings.objects);
        match row {
            SettingRow::BackgroundColor => Some(&mut bg.color),
            SettingRow::GradientFrom => Some(&mut bg.gradient_from),
            SettingRow::GradientTo => Some(&mut bg.gradient_to),
            SettingRow::BoxSpineColor => Some(&mut boxes.spine_color),
            SettingRow::BoxSpineText => Some(&mut boxes.spine_text_color),
            SettingRow::BoxTopColor => Some(&mut boxes.top_color),
            SettingRow::ObjectColor => Some(&mut objects.color),
            _ => None,
        }
    }

    pub(in crate::app) fn box_size_slot(row: SettingRow) -> Option<(usize, (f32, f32), f32)> {
        match row {
            SettingRow::BoxWidth => Some((0, theme::BOX_WIDTH_RANGE, 5.0)),
            SettingRow::BoxHeight => Some((1, theme::BOX_HEIGHT_RANGE, 5.0)),
            SettingRow::BoxDepth => Some((2, theme::BOX_DEPTH_RANGE, 1.0)),
            _ => None,
        }
    }

    pub(in crate::app) fn set_box_size(&mut self, row: SettingRow, mm: f32) {
        let Some((i, (min, max), _)) = Self::box_size_slot(row) else { return };
        let boxes = &mut self.themes[self.theme].settings.boxes;
        boxes.custom_size = boxes.size_mm();
        boxes.custom_size[i] = mm.round().clamp(min, max);
        self.save_theme();
    }

    pub(in crate::app) fn set_crt_size(&mut self, row: SettingRow, px: f32) {
        let crt = &mut self.themes[self.theme].settings.crt;
        let [w, h] = crt.size();
        let (min, max) = CrtSettings::SIZE_RANGE;
        let px = px.round().clamp(min, max);
        (crt.width, crt.height) = if row == SettingRow::CrtWidth { (px, h) } else { (w, px) };
        self.save_theme();
    }

    pub(in crate::app) fn button_color_slot(&mut self, row: SettingRow) -> Option<&mut Option<Color32>> {
        let SettingRow::Button(button, field) = row else { return None };
        let look = self.themes[self.theme].settings.buttons.get_mut(button);
        match field {
            ButtonRow::Color => Some(&mut look.color),
            ButtonRow::TextColor => Some(&mut look.text_color),
            _ => None,
        }
    }

    pub(in crate::app) fn game_color_slot(&mut self, row: SettingRow) -> Option<&mut Option<Color32>> {
        let o = &mut self.library.games.get_mut(self.selected)?.box_style;
        match row {
            SettingRow::GameSpineColor => Some(&mut o.spine_color),
            SettingRow::GameSpineText => Some(&mut o.spine_text_color),
            SettingRow::GameTopColor => Some(&mut o.top_color),
            SettingRow::GameObjectColor => Some(&mut o.object_color),
            _ => None,
        }
    }

    pub(in crate::app) fn activate_setting(&mut self, row: SettingRow) {
        if row.page().is_game() && self.selected_game().is_none() {
            return;
        }
        match row {
            SettingRow::Fullscreen => {
                self.settings.start_fullscreen ^= true;
                self.save_settings();
            }
            SettingRow::OrderLastPlayed => {
                if self.setting_enabled(row) {
                    self.settings.order.last_played_first ^= true;
                    self.save_settings();
                }
            }
            SettingRow::GalleryKeepAll => {
                self.settings.gallery.keep_all ^= true;
                self.library.prune_gallery = !self.settings.gallery.keep_all;
                self.save_settings();
            }
            SettingRow::OrderStatic if !self.settings.order.static_order => {
                self.mode = Mode::ConfirmRemove { what: Removal::StaticOrder, yes: false };
            }
            SettingRow::OrderStatic => {
                self.settings.order.static_order = false;
                self.save_settings();
                self.notify(tr!("Static order is off. Games keep their places until something moves them"));
            }
            SettingRow::Autostart => match autostart::set(!self.autostart) {
                Ok(()) => self.autostart = autostart::is_enabled(),
                Err(e) => self.notify(tr!("Could not change startup setting: {error}", error = format!("{e:#}"))),
            },
            SettingRow::TaskbarHide => {
                self.settings.windows.hide_taskbar ^= true;
                self.save_settings();
                self.sync_taskbar();
            }
            SettingRow::AppStoreUrl => {
                let buffer = self.settings.app_store().url.clone();
                self.mode = Mode::Prompt { kind: PromptKind::AppStoreUrl, buffer };
            }
            SettingRow::AppStoreAdd => {
                self.settings.add_app_store();
                self.save_settings();
                self.store_settings_changed();
                self.mode = Mode::Prompt { kind: PromptKind::AppStoreUrl, buffer: String::new() };
            }
            SettingRow::AppStoreRemove => self.mode = Mode::ConfirmRemove { what: Removal::AppStore, yes: false },
            SettingRow::ProfileActive => {
                if self.profile_pick == profiles::active() {
                    return self.adjust_setting(row, 1);
                }
                let name = self.profile_pick.clone();
                if !self.idle_or_notify() {
                    return;
                }
                match profiles::switch(&name) {
                    Ok(()) => self.restart(),
                    Err(e) => self.notify(tr!("Could not switch profile: {error}", error = format!("{e:#}"))),
                }
            }
            SettingRow::ProfileRename => self.mode = Mode::Prompt { kind: PromptKind::ProfileName, buffer: profiles::active() },
            SettingRow::ProfileNew => self.mode = Mode::Prompt { kind: PromptKind::ProfileNew, buffer: profiles::unique_name(profiles::DEFAULT) },
            SettingRow::ProfileExport => self.export_profile(),
            SettingRow::ProfileImport => self.import_profile(),
            SettingRow::ProfileReset => self.mode = Mode::ConfirmRemove { what: Removal::Profile, yes: false },
            SettingRow::AppReset => self.mode = Mode::ConfirmRemove { what: Removal::Everything, yes: false },
            SettingRow::AboutSource | SettingRow::AboutServer => {
                let url = if row == SettingRow::AboutSource { SOURCE_URL } else { SERVER_URL };
                if let Err(e) = open::that_detached(url) {
                    self.notify(tr!("Could not open {url}: {error}", url = url, error = e));
                }
            }
            SettingRow::AboutApp => {}
            SettingRow::AddonManager => self.mode = AddonManager::open(AddonView::Menu, 0),
            SettingRow::AppStoreTest => self.test_store_connection(),
            SettingRow::StoreEnabled(store) => {
                let s = self.settings.store_mut(store);
                s.enabled ^= true;
                if s.enabled && self.settings.store_command(store).is_none() {
                    self.notify(tr!("{store} was not found. Set its command below.", store = store.name()));
                }
                self.save_settings();
            }
            SettingRow::DiscScanAtStart => {
                self.settings.discs.scan_at_start ^= true;
                self.save_settings();
            }
            SettingRow::ControllerSwap => {
                self.settings.controls.swap_ab ^= true;
                self.controls_changed();
            }
            SettingRow::ControllerPrompts
            | SettingRow::ControllerButton(_)
            | SettingRow::SpecialPlace(_)
            | SettingRow::ButtonStyle
            | SettingRow::ButtonPick
            | SettingRow::Button(_, ButtonRow::Shape) => self.adjust_setting(row, 1),
            SettingRow::ControllerReset => {
                self.settings.controls.reset();
                self.controls_changed();
                self.mode = self.settings_at(SettingRow::ControllerSwap);
            }
            SettingRow::VirtualMouse if self.global_mouse.connected() => {
                self.notify(tr!("Yasaff Global Virtual Mouse handles the mouse while it runs"));
            }
            SettingRow::VirtualMouse => {
                self.settings.mouse.virtual_mouse ^= true;
                self.input.set_virtual_mouse(self.settings.mouse.virtual_mouse);
                self.save_settings();
            }
            SettingRow::MouseCursor => {
                let dialog = rfd::FileDialog::new().set_title(tr!("Select cursor image (its top-left corner is the tip)")).add_filter(tr!("Images"), IMAGE_EXTS);
                self.ask_path(dialog, Pick::File, |app, path| {
                    if let Err(e) = app.settings.import_cursor(&path) {
                        app.notify(tr!("Could not import cursor: {error}", error = format!("{e:#}")));
                    }
                });
            }
            SettingRow::FontImport => self.import_fonts(),
            SettingRow::FontManage => self.mode = Mode::FontManager { cursor: 0, armed: false },
            SettingRow::MouseCursorClear => {
                self.settings.clear_cursor();
                self.save_settings();
                self.mode = self.settings_at(SettingRow::MouseCursor);
            }
            SettingRow::DiscRemovable => {
                self.settings.discs.removable_drives ^= true;
                self.disc_removable.store(self.settings.discs.removable_drives, Ordering::Relaxed);
                self.save_settings();
            }
            SettingRow::DiscHideFileWindow => {
                self.settings.discs.hide_file_window ^= true;
                self.autoplay = None;
                if self.settings.discs.hide_file_window {
                    self.autoplay = crate::autoplay::suppress();
                }
                self.save_settings();
            }
            SettingRow::DiscKeepSaves => {
                self.settings.discs.keep_saves ^= true;
                self.save_settings();
            }
            SettingRow::DeviceCreateFile => self.create_disc_file(),
            SettingRow::DeviceBurn => self.open_burner(),
            SettingRow::DeviceInsertConfig => self.confirm_device_write(DeviceWrite::LaunchConfig),
            SettingRow::DeviceCreateCatalog => self.confirm_device_write(DeviceWrite::SaveCatalog),
            SettingRow::DeviceBackupNow => self.backup_saves(),
            SettingRow::DeviceOpenSaveFolder => self.open_save_dir(),
            SettingRow::DeviceSetSaveFolder => self.pick_save_dir(),
            SettingRow::DeviceSavePath(i) => self.remove_device_save_path(i),
            SettingRow::DeviceSaveFolder => {}
            SettingRow::DeviceAddFiles => self.add_device_save_paths(false),
            SettingRow::DeviceAddFolders => self.add_device_save_paths(true),
            SettingRow::DeviceTarget | SettingRow::DeviceKeepSaves | SettingRow::DeviceRecovery => self.adjust_device_setting(row, 1),
            SettingRow::StoreBackground(store) => {
                let s = self.settings.store_mut(store);
                s.start_in_background ^= true;
                if s.start_in_background {
                    self.start_store_in_background(store);
                }
                self.save_settings();
            }
            SettingRow::StoreCommand(store) => {
                let custom = self.settings.store(store).command.clone();
                let buffer = if custom.is_empty() {
                    store.detect().map(|c| apps::join_command(&c[0], &c[1..])).unwrap_or_default()
                } else {
                    custom
                };
                self.mode = Mode::Prompt { kind: PromptKind::StoreCommand(store), buffer };
            }
            SettingRow::ThemeRename => {
                let buffer = self.theme().settings.name.clone();
                self.mode = Mode::Prompt { kind: PromptKind::ThemeName, buffer };
            }
            SettingRow::ThemeNew => {
                let name = tr!("{name} copy", name = self.theme().settings.name);
                match self.theme().duplicate(name) {
                    Ok(theme) => {
                        let id = theme.id.clone();
                        self.themes.push(theme);
                        self.select_theme(&id);
                        self.notify(tr!("Created {name}", name = self.theme().settings.name));
                    }
                    Err(e) => self.notify(tr!("Could not create theme: {error}", error = format!("{e:#}"))),
                }
            }
            SettingRow::ThemeExport => {
                let name = &self.theme().settings.name;
                let dialog = rfd::FileDialog::new().set_title(tr!("Export theme {name}", name = name)).add_filter("ZIP", &["zip"]).set_file_name(self.theme().package_name());
                self.ask_path(dialog, Pick::Save, |app, file| match app.theme().export(&file) {
                    Ok(()) => app.notify(tr!("Theme exported to {file}", file = file.display())),
                    Err(e) => app.notify(tr!("Could not export theme: {error}", error = format!("{e:#}"))),
                });
            }
            SettingRow::ThemeImport => {
                let dialog = rfd::FileDialog::new().set_title(tr!("Select a theme package")).add_filter("ZIP", &["zip"]);
                self.ask_path(dialog, Pick::File, |app, zip| app.import_theme_zip(&zip));
            }
            SettingRow::ThemeDelete => {
                if self.theme().is_builtin() {
                    self.notify(tr!("{name} is a built-in theme and can't be deleted.", name = self.theme().settings.name));
                } else if !theme::deletable(&self.themes, self.theme) {
                    self.notify(tr!("This is your only own theme. Create another one before deleting it."));
                } else {
                    self.mode = Mode::ConfirmRemove { what: Removal::Theme, yes: false };
                }
            }
            SettingRow::BubbleSoundFile | SettingRow::SwapSoundFile => {
                let dialog = rfd::FileDialog::new().set_title(tr!("Select swap sound")).add_filter(tr!("Sounds"), crate::sound::EXTS);
                let bubbles = row == SettingRow::BubbleSoundFile;
                self.ask_path(dialog, Pick::File, move |app, path| {
                    if let Err(e) = app.themes[app.theme].set_sound_file(bubbles, &path) {
                        app.notify(tr!("Could not set sound: {error}", error = format!("{e:#}")));
                    } else {
                        app.play_swap_sound(bubbles);
                    }
                });
            }
            SettingRow::BackgroundImage => {
                let dialog = rfd::FileDialog::new().set_title(tr!("Select background image")).add_filter(tr!("Images"), IMAGE_EXTS);
                self.ask_path(dialog, Pick::File, |app, path| {
                    if let Err(e) = app.themes[app.theme].set_background_image(&path) {
                        app.notify(tr!("Could not set image: {error}", error = format!("{e:#}")));
                    }
                });
            }
            SettingRow::Bar(slot, BarRow::Image) => {
                let dialog = rfd::FileDialog::new().set_title(tr!("Select bar image")).add_filter(tr!("Images"), IMAGE_EXTS);
                self.ask_path(dialog, Pick::File, move |app, path| {
                    if let Err(e) = app.themes[app.theme].set_bar_image(slot, &path) {
                        app.notify(tr!("Could not set image: {error}", error = format!("{e:#}")));
                    }
                });
            }
            SettingRow::Button(button, ButtonRow::Icon) => {
                let dialog = rfd::FileDialog::new().set_title(tr!("Select button icon")).add_filter(tr!("Images"), IMAGE_EXTS);
                self.ask_path(dialog, Pick::File, move |app, path| {
                    if let Err(e) = app.themes[app.theme].set_button_icon(button, &path) {
                        app.notify(tr!("Could not set image: {error}", error = format!("{e:#}")));
                    }
                });
            }
            SettingRow::TileIcon(special) => {
                let dialog = rfd::FileDialog::new()
                    .set_title(tr!("Select an icon for {tile}", tile = special.name()))
                    .add_filter(tr!("Icons"), &["svg", "png", "jpg", "jpeg", "webp"]);
                self.ask_path(dialog, Pick::File, move |app, path| {
                    if let Err(e) = app.themes[app.theme].set_tile_icon(special.key(), &path) {
                        app.notify(tr!("Could not set image: {error}", error = format!("{e:#}")));
                    }
                });
            }
            SettingRow::TileIconClear(special) => {
                if let Err(e) = self.themes[self.theme].clear_tile_icon(special.key()) {
                    self.notify(tr!("Could not save theme: {error}", error = format!("{e:#}")));
                }
                self.mode = self.settings_at(SettingRow::TileIcon(special));
            }
            SettingRow::Button(button, ButtonRow::IconClear) => {
                if let Err(e) = self.themes[self.theme].clear_button_icon(button) {
                    self.notify(tr!("Could not save theme: {error}", error = format!("{e:#}")));
                }
                self.mode = self.settings_at(SettingRow::Button(button, ButtonRow::Icon));
            }
            SettingRow::Button(_, ButtonRow::Color | ButtonRow::TextColor) => {
                let buffer = self.button_color_slot(row).and_then(|c| *c).map(theme::to_hex).unwrap_or_default();
                self.mode = Mode::Prompt { kind: PromptKind::Color(row), buffer };
            }
            SettingRow::Bar(slot, BarRow::ImageReset) => {
                self.themes[self.theme].settings.bar_mut(slot).reset_image_placement();
                self.save_theme();
                self.mode = self.settings_at(SettingRow::Bar(slot, BarRow::ImageZoom));
            }
            SettingRow::Bar(_, BarRow::Color | BarRow::GradientFrom | BarRow::GradientTo)
            | SettingRow::BackgroundColor
            | SettingRow::GradientFrom
            | SettingRow::GradientTo
            | SettingRow::BoxSpineColor
            | SettingRow::BoxSpineText
            | SettingRow::BoxTopColor
            | SettingRow::ObjectColor => {
                let buffer = self.color_slot(row).map(|c| theme::to_hex(*c)).unwrap_or_default();
                self.mode = Mode::Prompt { kind: PromptKind::Color(row), buffer };
            }
            SettingRow::GameSpineColor | SettingRow::GameSpineText | SettingRow::GameTopColor | SettingRow::GameObjectColor => {
                let buffer = self.game_color_slot(row).and_then(|c| *c).map(theme::to_hex).unwrap_or_default();
                self.mode = Mode::Prompt { kind: PromptKind::Color(row), buffer };
            }
            SettingRow::CrtWidth | SettingRow::CrtHeight => {
                let [w, h] = self.theme().settings.crt.size();
                let buffer = format!("{:.0}", if row == SettingRow::CrtWidth { w } else { h });
                self.mode = Mode::Prompt { kind: PromptKind::CrtSize(row), buffer };
            }
            SettingRow::BoxWidth | SettingRow::BoxHeight | SettingRow::BoxDepth => {
                let Some((i, ..)) = Self::box_size_slot(row) else { return };
                let buffer = format!("{:.0}", self.theme().settings.boxes.size_mm()[i]);
                self.mode = Mode::Prompt { kind: PromptKind::BoxSize(row), buffer };
            }
            SettingRow::StyleCover(style) => self.choose_image(ImageSlot::Cover(style), row),
            SettingRow::StyleAdjust(style) => self.open_style_editor(style),
            SettingRow::StyleAdjustReset(style) => {
                self.library.games[self.selected].set_style_placement(style, CoverPlacement::default());
                self.save();
                self.mode = self.settings_at(SettingRow::StyleAdjust(style));
            }
            SettingRow::GalleryImage(i) => self.remove_gallery_image(i),
            SettingRow::GalleryEmpty => {}
            SettingRow::GameBackground => self.choose_image(ImageSlot::Asset(AssetKind::Background), row),
            SettingRow::GameAdjustReset(kind) => {
                *self.library.games[self.selected].placement_mut(kind) = CoverPlacement::default();
                self.save();
                self.mode = self.settings_at(SettingRow::GameAdjust(kind));
            }
            SettingRow::GameAdjust(kind) => self.open_cover_editor(kind),
            SettingRow::GameSpine => self.choose_image(ImageSlot::Asset(AssetKind::Spine), row),
            SettingRow::GameTop => self.choose_image(ImageSlot::Asset(AssetKind::Top), row),
            SettingRow::GameBack => self.choose_image(ImageSlot::Asset(AssetKind::Back), row),
            SettingRow::GameSide => self.choose_image(ImageSlot::Asset(AssetKind::Side), row),
            SettingRow::GameBottom => self.choose_image(ImageSlot::Asset(AssetKind::Bottom), row),
            SettingRow::GameBooklet => self.choose_image(ImageSlot::Asset(AssetKind::Booklet), row),
            SettingRow::GameModel => self.pick_asset(AssetKind::Model),
            SettingRow::GameVideo => self.pick_asset(AssetKind::Video),
            SettingRow::GameIdleVideo => self.pick_asset(AssetKind::IdleVideo),
            SettingRow::ThemeOption(i) => self.activate_theme_option(i),
            SettingRow::ThemePicture(slot) => {
                let dialog = rfd::FileDialog::new().set_title(tr!("Select image")).add_filter(tr!("Images"), IMAGE_EXTS);
                let Some(key) = self.picture_key(slot) else { return };
                self.ask_path(dialog, Pick::File, move |app, path| {
                    if let Err(e) = app.themes[app.theme].set_picture(&key, &path) {
                        app.notify(tr!("Could not set image: {error}", error = format!("{e:#}")));
                    }
                });
            }
            SettingRow::ThemePictureAdjust(slot) => self.open_picture_editor(slot),
            SettingRow::ThemePictureClear(slot) => {
                if let Some(key) = self.picture_key(slot)
                    && let Err(e) = self.themes[self.theme].clear_picture(&key)
                {
                    self.notify(tr!("Could not save theme: {error}", error = format!("{e:#}")));
                }
                self.mode = self.settings_at(SettingRow::ThemePicture(slot));
            }
            SettingRow::ThemePicturesEmpty => {}
            SettingRow::CategoryAdd => self.mode = Mode::Prompt { kind: PromptKind::CategoryName(None), buffer: String::new() },
            SettingRow::Category(i) => {
                let buffer = self.library.categories.get(i).cloned().unwrap_or_default();
                self.mode = Mode::Prompt { kind: PromptKind::CategoryName(Some(i)), buffer };
            }
            SettingRow::GameMainCategory | SettingRow::GameCategory(_) => self.adjust_setting(row, 1),
            SettingRow::GameNoCategories => {}
            SettingRow::GameThemePicture => self.choose_image(ImageSlot::Asset(AssetKind::ThemePicture), row),
            SettingRow::GameThemePictureClear => {
                self.library.clear_asset(self.selected, AssetKind::ThemePicture);
                self.library.games[self.selected].picture_seeded = true;
                self.save();
                self.mode = self.settings_at(SettingRow::GameThemePicture);
            }
            SettingRow::BubbleShuffle => {
                let bubbles = &mut self.themes[self.theme].settings.bubbles;
                let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
                bubbles.seed = bubbles.seed.wrapping_add(nanos | 1);
                self.save_theme();
            }
            SettingRow::GameSpineClear
            | SettingRow::GameTopClear
            | SettingRow::GameBackClear
            | SettingRow::GameSideClear
            | SettingRow::GameBottomClear
            | SettingRow::GameBookletClear
            | SettingRow::GameBackgroundClear
            | SettingRow::GameModelClear
            | SettingRow::GameVideoClear
            | SettingRow::GameIdleVideoClear => {
                let (kind, next) = match row {
                    SettingRow::GameVideoClear => (AssetKind::Video, SettingRow::GameVideo),
                    SettingRow::GameIdleVideoClear => (AssetKind::IdleVideo, SettingRow::GameIdleVideo),
                    SettingRow::GameSpineClear => (AssetKind::Spine, SettingRow::GameSpine),
                    SettingRow::GameTopClear => (AssetKind::Top, SettingRow::GameTop),
                    SettingRow::GameSideClear => (AssetKind::Side, SettingRow::GameSide),
                    SettingRow::GameBottomClear => (AssetKind::Bottom, SettingRow::GameBottom),
                    SettingRow::GameBookletClear => (AssetKind::Booklet, SettingRow::GameBooklet),
                    SettingRow::GameBackgroundClear => (AssetKind::Background, SettingRow::GameBackground),
                    SettingRow::GameModelClear => (AssetKind::Model, SettingRow::GameModel),
                    _ => (AssetKind::Back, SettingRow::GameBack),
                };
                self.library.clear_asset(self.selected, kind);
                self.save();
                self.mode = self.settings_at(next);
            }
            SettingRow::DiscOnInsert
            | SettingRow::Language
            | SettingRow::AppStoreActive
            | SettingRow::MouseSpeed
            | SettingRow::MouseCursorSize
            | SettingRow::FontTitle
            | SettingRow::GameTitleFont
            | SettingRow::Theme
            | SettingRow::ViewKind
            | SettingRow::TileStyle
            | SettingRow::BoxCase
            | SettingRow::BoxSpineLabel
            | SettingRow::GameSpineLabel
            | SettingRow::GameModelUp
            | SettingRow::TileRows
            | SettingRow::TileSize
            | SettingRow::TileSpacing
            | SettingRow::EnlargeSelected
            | SettingRow::SelectedScale
            | SettingRow::TileBar
            | SettingRow::GridColumns
            | SettingRow::GridRows
            | SettingRow::GridSpacing
            | SettingRow::GridEnlarge
            | SettingRow::GameArt
            | SettingRow::GridAlign
            | SettingRow::DiscKind
            | SettingRow::DiscAngle
            | SettingRow::DiscSpin
            | SettingRow::DiscSpinDirection
            | SettingRow::DiscSpinSpeed
            | SettingRow::BackgroundKind
            | SettingRow::GradientDirection
            | SettingRow::ImageFit
            | SettingRow::BubbleSize
            | SettingRow::BubbleVariance
            | SettingRow::BubbleFill
            | SettingRow::BubbleVideo
            | SettingRow::CrtVideo
            | SettingRow::CrtIdleVideo
            | SettingRow::CrtFilter
            | SettingRow::BubbleSound
            | SettingRow::BubbleCustomSound
            | SettingRow::SwapSound
            | SettingRow::SwapCustomSound
            | SettingRow::Bar(..) => self.adjust_setting(row, 1),
        }
    }

    pub(in crate::app) fn adjust_setting(&mut self, row: SettingRow, delta: i32) {
        if !self.setting_enabled(row) {
            return;
        }
        if row.page() == Page::GameBox {
            return self.adjust_game_setting(row, delta);
        }
        if row.page() == Page::GameDevice {
            return self.adjust_device_setting(row, delta);
        }
        if row.page() == Page::GameDetails {
            return self.adjust_details_setting(row, delta);
        }
        if row.page() == Page::ThemePictures {
            return;
        }
        if let SettingRow::ThemeOption(i) = row {
            return self.adjust_theme_option(i, delta);
        }
        match row {
            SettingRow::ControllerSwap | SettingRow::OrderLastPlayed | SettingRow::OrderStatic | SettingRow::GalleryKeepAll => {
                return self.activate_setting(row);
            }
            SettingRow::ControllerPrompts => {
                let choices = [None, Some(PadFamily::Xbox), Some(PadFamily::PlayStation)];
                self.settings.controls.prompts = cycle(&choices, self.settings.controls.prompts, delta);
                return self.save_settings();
            }
            SettingRow::ButtonPick => {
                self.button_edit = cycle(&PadButton::ALL, self.button_edit, delta);
                return;
            }
            SettingRow::ControllerButton(button) => {
                let was_custom = self.settings.controls.is_custom();
                let control = cycle(&Control::ALL, self.input.bindings().get(button), delta);
                self.settings.controls.bind(button, control);
                self.controls_changed();
                if was_custom != self.settings.controls.is_custom() {
                    self.mode = self.settings_at(row);
                }
                return;
            }
            SettingRow::SpecialPlace(special) => {
                let Some(place) = Self::special_place(&mut self.settings, special) else { return };
                *place = cycle(&[None, Some(BarSlot::Top), Some(BarSlot::Bottom)], *place, delta);
                self.clamp_selection();
                return self.save_settings();
            }
            SettingRow::Category(i) => {
                let to = self.library.move_category(i, delta);
                self.save();
                self.mode = self.settings_at(SettingRow::Category(to));
                return;
            }
            _ => {}
        }
        let d = delta as f32;
        let bar_minimum = BarSlot::ALL.map(|slot| self.bar_minimum(slot).map_or(0.0, |(h, _)| h));
        let settings = &mut self.themes[self.theme].settings;
        let (tiles, grid, bg, discs) = (&mut settings.tiles, &mut settings.grid, &mut settings.background, &mut settings.discs);
        match row {
            SettingRow::MouseSpeed => {
                let m = &mut self.settings.mouse;
                m.speed = step(m.speed, 100.0 * d, MouseSettings::SPEED_RANGE);
                return self.save_settings();
            }
            SettingRow::MouseCursorSize => {
                let m = &mut self.settings.mouse;
                m.cursor_size = step(m.cursor_size, 4.0 * d, MouseSettings::SIZE_RANGE);
                return self.save_settings();
            }
            SettingRow::FontTitle => {
                let choices: Vec<String> = std::iter::once(String::new()).chain(self.fonts.iter().cloned()).collect();
                let i = choices.iter().position(|f| *f == self.settings.fonts.title).unwrap_or(0) as i32;
                let next = choices[(i + delta).rem_euclid(choices.len() as i32) as usize].clone();
                return self.set_title_font(next);
            }
            SettingRow::Fullscreen
            | SettingRow::Autostart
            | SettingRow::VirtualMouse
            | SettingRow::StoreEnabled(_)
            | SettingRow::StoreBackground(_)
            | SettingRow::DiscScanAtStart
            | SettingRow::DiscRemovable
            | SettingRow::DiscHideFileWindow
            | SettingRow::DiscKeepSaves => {
                return self.activate_setting(row);
            }
            SettingRow::DiscOnInsert => {
                self.settings.discs.on_insert = cycle(&DiscAction::ALL, self.settings.discs.on_insert, delta);
                return self.save_settings();
            }
            SettingRow::Language => {
                self.settings.language = cycle(&Language::ALL, self.settings.language, delta);
                i18n::set(self.settings.language);
                return self.save_settings();
            }
            SettingRow::ProfileActive => {
                self.profiles = profiles::list();
                let n = self.profiles.len() as i32;
                if n > 0 {
                    let i = self.profiles.iter().position(|p| *p == self.profile_pick).unwrap_or(0) as i32;
                    self.profile_pick = self.profiles[(i + delta).rem_euclid(n) as usize].clone();
                }
                return;
            }
            SettingRow::AppStoreActive => {
                let n = self.settings.app_stores.len() as i32;
                if n < 2 {
                    return;
                }
                self.settings.active_app_store = (self.settings.active_app_store as i32 + delta).rem_euclid(n) as usize;
                self.save_settings();
                return self.store_settings_changed();
            }
            SettingRow::Theme => {
                let n = self.themes.len() as i32;
                let i = (self.theme as i32 + delta).rem_euclid(n) as usize;
                let id = self.themes[i].id.clone();
                return self.select_theme(&id);
            }
            SettingRow::ViewKind => {
                let next = cycle(&ViewKind::ALL, settings.view, delta);
                return self.set_view(next);
            }
            SettingRow::BubbleSize => {
                let b = &mut settings.bubbles;
                b.size = step(b.size, 10.0 * d, BubbleSettings::SIZE_RANGE);
            }
            SettingRow::BubbleVariance => {
                let b = &mut settings.bubbles;
                b.size_variance = step(b.size_variance, 5.0 * d, BubbleSettings::VARIANCE_RANGE);
            }
            SettingRow::BubbleVideo => settings.bubbles.play_video ^= true,
            SettingRow::CrtWidth | SettingRow::CrtHeight => {
                let [w, h] = settings.crt.size();
                let px = if row == SettingRow::CrtWidth { w } else { h } + 10.0 * d;
                return self.set_crt_size(row, px);
            }
            SettingRow::CrtVideo => settings.crt.play_video ^= true,
            SettingRow::CrtIdleVideo => settings.crt.idle_video ^= true,
            SettingRow::CrtFilter => settings.crt.filter ^= true,
            SettingRow::BubbleSound => settings.bubbles.sound ^= true,
            SettingRow::BubbleCustomSound => settings.bubbles.custom_sound ^= true,
            SettingRow::SwapSound => settings.swap.sound ^= true,
            SettingRow::SwapCustomSound => settings.swap.custom_sound ^= true,
            SettingRow::BubbleFill => settings.bubbles.fill_screen ^= true,
            SettingRow::TileStyle => settings.tile_style = cycle(&TileStyle::ALL, settings.tile_style, delta),
            SettingRow::BoxCase => settings.boxes.case = cycle(&BoxCase::ALL, settings.boxes.case, delta),
            SettingRow::BoxWidth | SettingRow::BoxHeight | SettingRow::BoxDepth => {
                let Some((i, _, by)) = Self::box_size_slot(row) else { return };
                let mm = settings.boxes.size_mm()[i] + by * d;
                return self.set_box_size(row, mm);
            }
            SettingRow::BoxSpineLabel => {
                settings.boxes.spine_label = cycle(&SpineLabel::ALL, settings.boxes.spine_label, delta)
            }
            SettingRow::TileRows => tiles.rows = step_count(tiles.rows, delta, TileSettings::ROWS_RANGE),
            SettingRow::TileSize => tiles.size = step(tiles.size, 10.0 * d, TileSettings::SIZE_RANGE),
            SettingRow::TileSpacing => tiles.spacing = step(tiles.spacing, 2.0 * d, TileSettings::SPACING_RANGE),
            SettingRow::EnlargeSelected => tiles.enlarge_selected ^= true,
            SettingRow::SelectedScale => {
                let scale = step(tiles.selected_scale, 0.05 * d, TileSettings::SCALE_RANGE);
                tiles.selected_scale = (scale * 100.0).round() / 100.0;
            }
            SettingRow::TileBar => tiles.bar_position = cycle(&BarPosition::ALL, tiles.bar_position, delta),
            SettingRow::GridColumns => grid.columns = step_count(grid.columns, delta, GridSettings::COLUMNS_RANGE),
            SettingRow::GridRows => grid.rows = step_count(grid.rows, delta, GridSettings::ROWS_RANGE),
            SettingRow::GridSpacing => grid.spacing = step(grid.spacing, 2.0 * d, GridSettings::SPACING_RANGE),
            SettingRow::GridEnlarge => grid.enlarge_selected ^= true,
            SettingRow::GameArt => bg.game_art ^= true,
            SettingRow::DiscKind => discs.kind = cycle(&DiscKind::ALL, discs.kind, delta),
            SettingRow::DiscAngle => discs.angle = step(discs.angle, 5.0 * d, DiscSettings::ANGLE_RANGE),
            SettingRow::DiscSpin => discs.spin ^= true,
            SettingRow::DiscSpinDirection => discs.spin_direction = cycle(&DiscSpin::ALL, discs.spin_direction, delta),
            SettingRow::DiscSpinSpeed => {
                let by = match discs.spin_speed + d.min(0.0) {
                    ..10.0 => 1.0,
                    ..60.0 => 5.0,
                    _ => 20.0,
                };
                discs.spin_speed = step(discs.spin_speed, by * d, DiscSettings::SPEED_RANGE);
            }
            SettingRow::GridAlign => grid.align = cycle(&GridAlign::ALL, grid.align, delta),
            SettingRow::BackgroundKind => bg.kind = cycle(&BackgroundKind::ALL, bg.kind, delta),
            SettingRow::GradientDirection => {
                bg.gradient_direction = cycle(&GradientDirection::ALL, bg.gradient_direction, delta)
            }
            SettingRow::ImageFit => bg.image_fit = cycle(&ImageFit::ALL, bg.image_fit, delta),
            SettingRow::Bar(slot, field @ (BarRow::Enabled | BarRow::Height | BarRow::Kind | BarRow::GradientDirection | BarRow::ImageZoom | BarRow::ImageX | BarRow::ImageY | BarRow::ImageRotation)) => {
                let bar = settings.bar_mut(slot);
                match field {
                    BarRow::Enabled => bar.enabled ^= true,
                    BarRow::Height => {
                        let min = bar_minimum[slot as usize];
                        bar.height = Some(step(bar.height(slot).max(min), 4.0 * d, BarStyle::HEIGHT_RANGE).max(min));
                    }
                    BarRow::Kind => bar.kind = cycle(&BackgroundKind::ALL, bar.kind, delta),
                    BarRow::GradientDirection => bar.gradient_direction = cycle(&GradientDirection::ALL, bar.gradient_direction, delta),
                    BarRow::ImageZoom => bar.image_zoom = snap(step(bar.image_zoom, 0.05 * d, BarStyle::ZOOM_RANGE)),
                    BarRow::ImageX => bar.image_x = snap(step(bar.image_x, 0.02 * d, BarStyle::OFFSET_RANGE)),
                    BarRow::ImageY => bar.image_y = snap(step(bar.image_y, 0.05 * d, BarStyle::OFFSET_RANGE)),
                    _ => bar.image_rotation = (bar.image_rotation + 5.0 * d + 180.0).rem_euclid(360.0) - 180.0,
                }
            }
            SettingRow::ButtonStyle => {
                let buttons = &mut settings.buttons;
                buttons.set_style(cycle(&ButtonStyle::ALL, buttons.style(), delta));
            }
            SettingRow::Button(button, ButtonRow::Shape) => {
                let look = settings.buttons.get_mut(button);
                look.shape = Some(cycle(&ButtonShape::ALL, look.shape.unwrap_or_default(), delta));
            }
            SettingRow::Button(_, ButtonRow::Color | ButtonRow::TextColor) => {
                let Some(slot) = self.button_color_slot(row) else { return };
                *slot = cycle_or_default(BUTTON_PRESETS, *slot, delta);
            }
            SettingRow::Bar(_, BarRow::Color | BarRow::GradientFrom | BarRow::GradientTo)
            | SettingRow::BackgroundColor
            | SettingRow::GradientFrom
            | SettingRow::GradientTo
            | SettingRow::BoxSpineColor
            | SettingRow::BoxSpineText
            | SettingRow::BoxTopColor
            | SettingRow::ObjectColor => {
                let Some(color) = self.color_slot(row) else { return };
                *color = match theme::COLOR_PRESETS.iter().position(|c| c == color) {
                    Some(_) => cycle(theme::COLOR_PRESETS, *color, delta),
                    None if delta < 0 => *theme::COLOR_PRESETS.last().unwrap(),
                    None => theme::COLOR_PRESETS[0],
                };
            }
            _ => return,
        }
        self.save_theme();
    }

    pub(in crate::app) fn set_view(&mut self, view: ViewKind) {
        let settings = &mut self.themes[self.theme].settings;
        let old = settings.view;
        settings.view = view;
        let fresh = match view {
            ViewKind::Row => settings.tiles == TileSettings::default(),
            ViewKind::Grid => settings.grid == GridSettings::default(),
        };
        let note = if fresh {
            tr!(
                "{old} setup saved. Switching back restores it. Now editing {new}, starting from a blank setup.",
                old = old.label(),
                new = view.label(),
            )
        } else {
            tr!("{old} setup saved. Switching back restores it. Now editing {new}.", old = old.label(), new = view.label())
        };
        self.save_theme();
        self.notify(note);
    }

    pub(in crate::app) fn adjust_game_setting(&mut self, row: SettingRow, delta: i32) {
        let Some(game) = self.library.games.get_mut(self.selected) else { return };
        let o = &mut game.box_style;
        match row {
            SettingRow::GameSpineLabel => o.spine_label = cycle_or_default(&SpineLabel::ALL, o.spine_label, delta),
            SettingRow::GameModelUp => game.model_up = cycle(&object3d::ModelUp::ALL, game.model_up, delta),
            SettingRow::GameTitleFont => {
                let choices: Vec<Option<String>> = std::iter::once(None).chain(self.fonts.iter().cloned().map(Some)).collect();
                let i = choices.iter().position(|f| *f == game.title_font).unwrap_or(0) as i32;
                game.title_font = choices[(i + delta).rem_euclid(choices.len() as i32) as usize].clone();
            }
            SettingRow::GameSpineColor | SettingRow::GameSpineText | SettingRow::GameTopColor | SettingRow::GameObjectColor => {
                let Some(slot) = self.game_color_slot(row) else { return };
                *slot = cycle_or_default(theme::COLOR_PRESETS, *slot, delta);
            }
            _ => return,
        }
        self.save();
    }

    pub(in crate::app) fn adjust_details_setting(&mut self, row: SettingRow, delta: i32) {
        let n = self.library.categories.len();
        let Some(game) = self.selected_game().filter(|_| n > 0) else { return };
        match row {
            SettingRow::GameMainCategory => {
                let current = game.main_category.as_deref().and_then(|m| self.library.categories.iter().position(|c| c == m));
                let next = match current {
                    Some(i) => (i as i32 + delta).rem_euclid(n as i32) as usize,
                    None if delta < 0 => n - 1,
                    None => 0,
                };
                self.library.set_main_category(self.selected, next);
            }
            SettingRow::GameCategory(i) if i < n => self.library.toggle_category(self.selected, i),
            _ => return,
        }
        self.save();
    }

    pub(in crate::app) fn adjust_device_setting(&mut self, row: SettingRow, delta: i32) {
        if row == SettingRow::DeviceTarget {
            return self.cycle_device(delta);
        }
        let Some(game) = self.library.games.get_mut(self.selected) else { return };
        let device = &mut game.device_saves;
        match row {
            SettingRow::DeviceKeepSaves => device.keep_on_device ^= true,
            SettingRow::DeviceRecovery => {
                device.recovery_count = step_count(device.recovery_count, delta, (0, crate::device_saves::MAX_RECOVERY));
            }
            _ => return,
        }
        self.save();
    }
}
