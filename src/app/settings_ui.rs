//! The Settings screen: its pages and rows, what each row shows, and what Left/Right and Confirm do on it.

use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum SettingRow {
    Fullscreen,
    Autostart,
    Language,
    ProfileActive,
    ProfileRename,
    ProfileNew,
    ProfileExport,
    ProfileImport,
    ProfileReset,
    AppReset,
    AppStoreActive,
    AppStoreUrl,
    AppStoreKey,
    AppStoreTest,
    AppStoreAdd,
    AppStoreRemove,
    VirtualMouse,
    MouseSpeed,
    MouseCursor,
    MouseCursorClear,
    MouseCursorSize,
    ControllerSwap,
    ControllerButton(PadButton),
    ControllerReset,
    SpecialPlace(Special),
    FontTitle,
    FontImport,
    FontManage,
    Theme,
    ThemeRename,
    ThemeNew,
    ThemeExport,
    ThemeImport,
    ThemeDelete,
    ViewKind,
    TileStyle,
    TileRows,
    TileSize,
    TileSpacing,
    EnlargeSelected,
    SelectedScale,
    TileBar,
    GridColumns,
    GridRows,
    GridSpacing,
    GridEnlarge,
    GridAlign,
    BubbleSize,
    BubbleVariance,
    BubbleFill,
    BubbleShuffle,
    BubbleVideo,
    BubbleSound,
    BubbleCustomSound,
    BubbleSoundFile,
    SwapSound,
    SwapCustomSound,
    SwapSoundFile,
    BoxCase,
    BoxWidth,
    BoxHeight,
    BoxDepth,
    BoxSpineLabel,
    BoxSpineColor,
    BoxSpineText,
    BoxTopColor,
    ObjectColor,
    DiscKind,
    DiscAngle,
    DiscSpin,
    DiscSpinDirection,
    DiscSpinSpeed,
    GameArt,
    BackgroundKind,
    BackgroundColor,
    GradientFrom,
    GradientTo,
    GradientDirection,
    BackgroundImage,
    ImageFit,
    Bar(BarSlot, BarRow),
    Button(FaceButton, ButtonRow),
    StoreEnabled(Store),
    StoreBackground(Store),
    StoreCommand(Store),
    DiscOnInsert,
    DiscScanAtStart,
    DiscRemovable,
    GameCover,
    GameAdjust(AssetKind),
    GameAdjustReset(AssetKind),
    GameSpine,
    GameSpineClear,
    GameTop,
    GameTopClear,
    GameBack,
    GameBackClear,
    GameSide,
    GameSideClear,
    GameBottom,
    GameBottomClear,
    GameBooklet,
    GameBookletClear,
    GameModel,
    GameModelUp,
    GameModelClear,
    GameVideo,
    GameVideoClear,
    GameObjectColor,
    GameSpineLabel,
    GameSpineColor,
    GameSpineText,
    GameTopColor,
    GameTitleFont,
    AddonBar(usize),
    AddonRemove(usize),
    AddonImport,
    AboutApp,
    AboutSource,
    AboutServer,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum BarRow {
    Enabled,
    Height,
    Kind,
    Color,
    GradientFrom,
    GradientTo,
    GradientDirection,
    Image,
    ImageZoom,
    ImageX,
    ImageY,
    ImageRotation,
    ImageReset,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ButtonRow {
    Color,
    TextColor,
    Icon,
    IconClear,
}

pub(super) const SOURCE_URL: &str = "https://github.com/naisyrkdev/YASAFF";

pub(super) const SERVER_URL: &str = "https://github.com/naisyrkdev/YasaffStuffServer";

impl SettingRow {
    pub(super) fn page(self) -> Page {
        use SettingRow as R;
        match self {
            R::GameCover
            | R::GameAdjust(_)
            | R::GameAdjustReset(_)
            | R::GameSpine
            | R::GameSpineClear
            | R::GameTop
            | R::GameTopClear
            | R::GameBack
            | R::GameBackClear
            | R::GameSide
            | R::GameSideClear
            | R::GameBottom
            | R::GameBottomClear
            | R::GameBooklet
            | R::GameBookletClear
            | R::GameModel
            | R::GameModelUp
            | R::GameModelClear
            | R::GameVideo
            | R::GameVideoClear
            | R::GameObjectColor
            | R::GameSpineLabel
            | R::GameSpineColor
            | R::GameSpineText
            | R::GameTopColor
            | R::GameTitleFont => Page::GameBox,
            _ => Page::App,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Page {
    App,
    GameBox,
}

pub(super) struct SettingSection {
    pub(super) title: &'static str,
    pub(super) status: Option<(String, Color32)>,
    pub(super) rows: Range<usize>,
}

pub(super) enum RowValue {
    Toggle(bool),
    Choice(String),
    Color(Color32),
    DefaultColor(Color32),
    Action(&'static str),
    Summary(String),
}

pub(super) fn section_of(sections: &[SettingSection], row: usize) -> usize {
    sections.iter().position(|sec| sec.rows.contains(&row)).unwrap_or(0)
}

pub(super) fn follow_scroll(ui: &Ui, id: Id, top: f32, h: f32, content_h: f32, view_h: f32) -> f32 {
    let mut scroll: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    scroll = scroll.min(top).max(top + h - view_h);
    scroll = scroll.clamp(0.0, (content_h - view_h).max(0.0));
    ui.data_mut(|d| d.insert_temp(id, scroll));
    ui.ctx().animate_value_with_time(id, scroll, 0.12)
}

pub(super) fn wheel_lines(ui: &Ui) -> f32 {
    ui.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::MouseWheel { unit, delta, .. } => Some(match unit {
                    egui::MouseWheelUnit::Point => delta.y / 40.0,
                    _ => delta.y,
                }),
                _ => None,
            })
            .sum()
    })
}

pub(super) fn value_arrows(painter: &Painter, row: Rect, right: f32, inner: f32, active: bool, s: f32) -> [Rect; 2] {
    let color = if active { Color32::WHITE } else { Color32::from_gray(110) };
    let (w, h) = (8.0 * s, 14.0 * s);
    let y = row.center().y;
    let more_x = right - w;
    let less_x = right - 26.0 * s - inner - 18.0 * s;
    for (x, dir) in [(less_x, -1.0), (more_x, 1.0)] {
        let tip = pos2(x + if dir > 0.0 { w } else { 0.0 }, y);
        let back = x + if dir > 0.0 { 0.0 } else { w };
        painter.add(egui::Shape::convex_polygon(vec![tip, pos2(back, y - h / 2.0), pos2(back, y + h / 2.0)], color, Stroke::NONE));
    }
    let hit = vec2(34.0 * s, row.height());
    [
        Rect::from_center_size(pos2(less_x + w / 2.0, y), hit),
        Rect::from_center_size(pos2(more_x + w / 2.0, y), hit),
    ]
}

pub(super) fn step(value: f32, delta: f32, (min, max): (f32, f32)) -> f32 {
    (value + delta).clamp(min, max)
}

pub(super) fn snap(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

pub(super) fn position_label(t: f32, start: &str, end: &str) -> String {
    match (t * 100.0).round() as i32 {
        0 => start.to_owned(),
        50 => tr!("Center").to_owned(),
        100 => end.to_owned(),
        p => format!("{p} %"),
    }
}

pub(super) fn step_count(value: u32, delta: i32, (min, max): (u32, u32)) -> u32 {
    value.saturating_add_signed(delta).clamp(min, max)
}

pub(super) fn cycle<T: Copy + PartialEq>(all: &[T], current: T, delta: i32) -> T {
    let i = all.iter().position(|x| *x == current).unwrap_or(0) as i32;
    all[(i + delta).rem_euclid(all.len() as i32) as usize]
}

pub(super) fn cycle_or_default<T: Copy + PartialEq>(all: &[T], current: Option<T>, delta: i32) -> Option<T> {
    let i = current.and_then(|c| all.iter().position(|x| *x == c)).map_or(0, |i| i as i32 + 1);
    let next = (i + delta).rem_euclid(all.len() as i32 + 1);
    (next > 0).then(|| all[next as usize - 1])
}

impl App {
    pub(super) fn setting_rows(&self, page: Page) -> Vec<SettingRow> {
        use SettingRow as R;
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
            rows.push(R::GameObjectColor);
            rows.extend([R::GameSpineLabel, R::GameSpineColor, R::GameSpineText, R::GameTopColor, R::GameTitleFont]);
            return rows;
        }
        let theme = &self.theme().settings;
        let mut rows = vec![R::Fullscreen, R::Autostart, R::Language];
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
                rows.extend([R::TileRows, R::TileSize, R::TileSpacing, R::EnlargeSelected]);
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
        for button in FaceButton::ALL {
            let button_rows: &[ButtonRow] = match self.theme().button_icon(button) {
                Some(_) => &[ButtonRow::Icon, ButtonRow::IconClear],
                None => &[ButtonRow::Color, ButtonRow::TextColor, ButtonRow::Icon],
            };
            rows.extend(button_rows.iter().map(|&r| R::Button(button, r)));
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
        }
        rows.extend([R::AppStoreActive, R::AppStoreUrl, R::AppStoreKey, R::AppStoreTest, R::AppStoreAdd]);
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
        rows.extend([R::DiscOnInsert, R::DiscScanAtStart, R::DiscRemovable]);
        for i in 0..self.addons.installed.len() {
            rows.extend([R::AddonBar(i), R::AddonRemove(i)]);
        }
        rows.push(R::AddonImport);
        rows.extend([R::AboutApp, R::AboutSource, R::AboutServer, R::AppReset]);
        rows
    }

    pub(super) fn setting_enabled(&self, row: SettingRow) -> bool {
        match row {
            SettingRow::TileBar => self.theme().settings.tiles.rows <= 1,
            SettingRow::ViewKind | SettingRow::TileStyle => !self.is_bubbles(),
            SettingRow::DiscAngle => !self.theme().settings.discs.lying_flat(),
            SettingRow::SpecialPlace(special) => self.special_button(special).is_some(),
            _ => true,
        }
    }

    pub(super) fn settings_cursor(&self, row: SettingRow) -> usize {
        self.setting_rows(row.page()).iter().position(|r| *r == row).unwrap_or(0)
    }

    pub(super) fn settings_at(&self, row: SettingRow) -> Mode {
        Mode::Settings { cursor: self.settings_cursor(row), page: row.page(), in_rows: true }
    }

    pub(super) fn color_slot(&mut self, row: SettingRow) -> Option<&mut Color32> {
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

    pub(super) fn box_size_slot(row: SettingRow) -> Option<(usize, (f32, f32), f32)> {
        match row {
            SettingRow::BoxWidth => Some((0, theme::BOX_WIDTH_RANGE, 5.0)),
            SettingRow::BoxHeight => Some((1, theme::BOX_HEIGHT_RANGE, 5.0)),
            SettingRow::BoxDepth => Some((2, theme::BOX_DEPTH_RANGE, 1.0)),
            _ => None,
        }
    }

    pub(super) fn set_box_size(&mut self, row: SettingRow, mm: f32) {
        let Some((i, (min, max), _)) = Self::box_size_slot(row) else { return };
        let boxes = &mut self.themes[self.theme].settings.boxes;
        boxes.custom_size = boxes.size_mm();
        boxes.custom_size[i] = mm.round().clamp(min, max);
        self.save_theme();
    }

    pub(super) fn button_color_slot(&mut self, row: SettingRow) -> Option<&mut Option<Color32>> {
        let SettingRow::Button(button, field) = row else { return None };
        let look = self.themes[self.theme].settings.buttons.get_mut(button);
        match field {
            ButtonRow::Color => Some(&mut look.color),
            ButtonRow::TextColor => Some(&mut look.text_color),
            _ => None,
        }
    }

    pub(super) fn game_color_slot(&mut self, row: SettingRow) -> Option<&mut Option<Color32>> {
        let o = &mut self.library.games.get_mut(self.selected)?.box_style;
        match row {
            SettingRow::GameSpineColor => Some(&mut o.spine_color),
            SettingRow::GameSpineText => Some(&mut o.spine_text_color),
            SettingRow::GameTopColor => Some(&mut o.top_color),
            SettingRow::GameObjectColor => Some(&mut o.object_color),
            _ => None,
        }
    }

    pub(super) fn activate_setting(&mut self, row: SettingRow) {
        if row.page() == Page::GameBox && self.selected_game().is_none() {
            return;
        }
        match row {
            SettingRow::Fullscreen => {
                self.settings.start_fullscreen ^= true;
                self.save_settings();
            }
            SettingRow::Autostart => match autostart::set(!self.autostart) {
                Ok(()) => self.autostart = autostart::is_enabled(),
                Err(e) => self.notify(tr!("Could not change startup setting: {error}", error = format!("{e:#}"))),
            },
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
            SettingRow::AddonBar(_) => self.adjust_setting(row, 1),
            SettingRow::AddonRemove(i) => self.mode = Mode::ConfirmRemove { what: Removal::Addon(i), yes: false },
            SettingRow::AddonImport => self.import_app_file(),
            SettingRow::AppStoreKey => self.mode = Mode::Prompt { kind: PromptKind::AppStoreKey, buffer: String::new() },
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
            SettingRow::ControllerButton(_) | SettingRow::SpecialPlace(_) => self.adjust_setting(row, 1),
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
                let dialog = rfd::FileDialog::new().set_title(tr!("Export theme to folder"));
                self.ask_path(dialog, Pick::Folder, |app, parent| match app.theme().export(&parent) {
                    Ok(dir) => app.notify(tr!("Theme exported to {folder}", folder = dir.display())),
                    Err(e) => app.notify(tr!("Could not export theme: {error}", error = format!("{e:#}"))),
                });
            }
            SettingRow::ThemeImport => {
                let dialog = rfd::FileDialog::new().set_title(tr!("Select an exported theme folder"));
                self.ask_path(dialog, Pick::Folder, |app, dir| match Theme::import(&dir) {
                    Ok(theme) => {
                        let id = theme.id.clone();
                        app.themes.push(theme);
                        app.select_theme(&id);
                        app.notify(tr!("Imported {name}", name = app.theme().settings.name));
                    }
                    Err(e) => app.notify(tr!("Could not import theme: {error}", error = format!("{e:#}"))),
                });
            }
            SettingRow::ThemeDelete => {
                if self.theme().is_builtin() {
                    self.notify(tr!("Bubbles is a built-in theme and can't be deleted."));
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
            SettingRow::BoxWidth | SettingRow::BoxHeight | SettingRow::BoxDepth => {
                let Some((i, ..)) = Self::box_size_slot(row) else { return };
                let buffer = format!("{:.0}", self.theme().settings.boxes.size_mm()[i]);
                self.mode = Mode::Prompt { kind: PromptKind::BoxSize(row), buffer };
            }
            SettingRow::GameCover => self.pick_asset(AssetKind::Cover),
            SettingRow::GameAdjustReset(kind) => {
                *self.library.games[self.selected].placement_mut(kind) = CoverPlacement::default();
                self.save();
                self.mode = self.settings_at(SettingRow::GameAdjust(kind));
            }
            SettingRow::GameAdjust(kind) => {
                let draft = self.library.games[self.selected].placement(kind);
                self.mode = Mode::CoverEditor(CoverEditor { kind, draft, cursor: 0, button: EditorButton::Accept });
            }
            SettingRow::GameSpine => self.pick_asset(AssetKind::Spine),
            SettingRow::GameTop => self.pick_asset(AssetKind::Top),
            SettingRow::GameBack => self.pick_asset(AssetKind::Back),
            SettingRow::GameSide => self.pick_asset(AssetKind::Side),
            SettingRow::GameBottom => self.pick_asset(AssetKind::Bottom),
            SettingRow::GameBooklet => self.pick_asset(AssetKind::Booklet),
            SettingRow::GameModel => self.pick_asset(AssetKind::Model),
            SettingRow::GameVideo => self.pick_asset(AssetKind::Video),
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
            | SettingRow::GameModelClear
            | SettingRow::GameVideoClear => {
                let (kind, next) = match row {
                    SettingRow::GameVideoClear => (AssetKind::Video, SettingRow::GameVideo),
                    SettingRow::GameSpineClear => (AssetKind::Spine, SettingRow::GameSpine),
                    SettingRow::GameTopClear => (AssetKind::Top, SettingRow::GameTop),
                    SettingRow::GameSideClear => (AssetKind::Side, SettingRow::GameSide),
                    SettingRow::GameBottomClear => (AssetKind::Bottom, SettingRow::GameBottom),
                    SettingRow::GameBookletClear => (AssetKind::Booklet, SettingRow::GameBooklet),
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
            | SettingRow::BubbleSound
            | SettingRow::BubbleCustomSound
            | SettingRow::SwapSound
            | SettingRow::SwapCustomSound
            | SettingRow::Bar(..) => self.adjust_setting(row, 1),
        }
    }

    pub(super) fn adjust_setting(&mut self, row: SettingRow, delta: i32) {
        if !self.setting_enabled(row) {
            return;
        }
        if row.page() == Page::GameBox {
            return self.adjust_game_setting(row, delta);
        }
        match row {
            SettingRow::ControllerSwap => return self.activate_setting(row),
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
            | SettingRow::DiscRemovable => {
                return self.activate_setting(row);
            }
            SettingRow::DiscOnInsert => {
                self.settings.discs.on_insert = cycle(&DiscAction::ALL, self.settings.discs.on_insert, delta);
                return self.save_settings();
            }
            SettingRow::AddonBar(i) => {
                let Some(id) = self.addons.installed.get(i).map(|m| m.id.clone()) else { return };
                let places = [None, Some(BarSlot::Top), Some(BarSlot::Bottom)];
                let entry = self.settings.addons.entry(id).or_default();
                entry.bar = cycle(&places, entry.bar, delta);
                self.addons.sync(&self.settings);
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

    pub(super) fn set_view(&mut self, view: ViewKind) {
        let settings = &mut self.themes[self.theme].settings;
        let old = settings.view;
        settings.view = view;
        let fresh = match view {
            ViewKind::Row => settings.tiles == TileSettings::default(),
            ViewKind::Grid => settings.grid == GridSettings::default(),
        };
        let note = if fresh {
            tr!(
                "{old} setup saved; switching back restores it. Now editing {new}, starting from a blank setup.",
                old = old.label(),
                new = view.label(),
            )
        } else {
            tr!("{old} setup saved; switching back restores it. Now editing {new}.", old = old.label(), new = view.label())
        };
        self.save_theme();
        self.notify(note);
    }

    pub(super) fn adjust_game_setting(&mut self, row: SettingRow, delta: i32) {
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

    pub(super) fn setting_heading(&self, row: SettingRow) -> Option<(&'static str, Option<(String, Color32)>)> {
        match row {
            SettingRow::Fullscreen => Some((tr!("General"), None)),
            SettingRow::ProfileActive => Some((
                tr!("Profiles"),
                Some((tr!("Profile: {name}", name = profiles::active()), Color32::from_gray(140))),
            )),
            SettingRow::AppReset => Some((tr!("Power options"), None)),
            SettingRow::AppStoreActive => Some((tr!("YASAFF Store"), Some(self.store_status()))),
            SettingRow::AboutApp => Some((tr!("About"), None)),
            SettingRow::AddonBar(0) => Some(self.addons_heading()),
            SettingRow::AddonImport if self.addons.installed.is_empty() => Some(self.addons_heading()),
            SettingRow::VirtualMouse => Some((tr!("Mouse"), self.global_mouse_status())),
            SettingRow::ControllerSwap => Some((tr!("Controller"), Some(self.controller_warning()))),
            SettingRow::ControllerButton(PadButton::South) if self.settings.controls.is_custom() => {
                Some((tr!("Controller"), Some(self.controller_warning())))
            }
            SettingRow::SpecialPlace(Special::Store) => Some((
                tr!("Store and Add game"),
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
            SettingRow::Button(FaceButton::A, ButtonRow::Color) => Some(self.buttons_heading()),
            SettingRow::Button(FaceButton::A, ButtonRow::Icon) if self.theme().button_icon(FaceButton::A).is_some() => {
                Some(self.buttons_heading())
            }
            SettingRow::DiscKind => Some((tr!("Discs"), Some((tr!("Covers are printed on the label side").to_owned(), Color32::from_gray(140))))),
            SettingRow::ObjectColor => Some((
                tr!("3D objects"),
                Some((tr!("Games without a 3D model show their name in 3D letters").to_owned(), Color32::from_gray(140))),
            )),
            SettingRow::BubbleSize => Some((tr!("Bubbles"), Some((tr!("Every game on one screen").to_owned(), Color32::from_gray(140))))),
            SettingRow::GameVideo => Some((tr!("Bubble video"), Some((tr!("Plays in the selected bubble").to_owned(), Color32::from_gray(140))))),
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
            _ => None,
        }
    }

    pub(super) fn addons_heading(&self) -> (&'static str, Option<(String, Color32)>) {
        (tr!("Addons"), Some((tr!("{count} installed", count = self.addons.installed.len()), Color32::from_gray(140))))
    }

    pub(super) fn buttons_heading(&self) -> (&'static str, Option<(String, Color32)>) {
        (tr!("Buttons"), Some((tr!("How A, B, X and Y look in the button hints").to_owned(), Color32::from_gray(140))))
    }

    pub(super) fn setting_sections(&self, rows: &[SettingRow]) -> Vec<SettingSection> {
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

    pub(super) fn setting_row_content(&self, row: SettingRow) -> (String, &'static str, RowValue) {
        use RowValue::{Action, Choice, Toggle};
        let theme = &self.theme().settings;
        let (tiles, grid, bg, boxes) = (&theme.tiles, &theme.grid, &theme.background, &theme.boxes);
        let px = |v: f32| Choice(format!("{v:.0} px"));
        let game = self.selected_game();
        let o = game.map(|g| g.box_style.clone()).unwrap_or_default();
        let has = |image: fn(&Game) -> bool| game.is_some_and(image);
        let game_color = |color: Option<Color32>, theme: Color32| match color {
            Some(c) => RowValue::Color(c),
            None => RowValue::DefaultColor(theme),
        };
        let game_color_sub = tr!("Enter to type a hex value; empty uses the theme's");
        match row {
            SettingRow::Fullscreen => (tr!("Start in fullscreen").into(), "", Toggle(self.settings.start_fullscreen)),
            SettingRow::Autostart => (tr!("Run at system startup").into(), tr!("Start YASAFF when you log in"), Toggle(self.autostart)),
            SettingRow::Language => {
                let label = match self.settings.language {
                    Language::English => "Language".to_owned(),
                    _ => format!("{} · Language", tr!("Language")),
                };
                (label, tr!("Left/Right switches between languages"), Choice(self.settings.language.label().into()))
            }
            SettingRow::AppStoreActive => {
                let stores = &self.settings.app_stores;
                let (sub, value) = match stores.len() {
                    0 | 1 => (tr!("Add another store below to switch between stores"), self.settings.app_store().title()),
                    n => (
                        tr!("Left/Right switches the store the Store tile shows"),
                        tr!("{store} ({index} of {count})", store = self.settings.app_store().title(), index = self.settings.active_app_store.min(n - 1) + 1, count = n),
                    ),
                };
                (tr!("Store").into(), sub, Choice(value))
            }
            SettingRow::ProfileActive => {
                let n = self.profiles.len();
                let i = self.profiles.iter().position(|p| *p == self.profile_pick).unwrap_or(0);
                let (sub, value) = match (n, self.profile_pick == profiles::active()) {
                    (0 | 1, _) => (tr!("Create another profile below to switch between them"), self.profile_pick.clone()),
                    (_, true) => (tr!("Left/Right picks a profile; Enter switches to it and restarts YASAFF"), tr!("{name} ({index} of {count})", name = self.profile_pick, index = i + 1, count = n)),
                    (_, false) => (tr!("Press Enter to switch; YASAFF restarts"), tr!("{name} ({index} of {count})", name = self.profile_pick, index = i + 1, count = n)),
                };
                (tr!("Change profile").into(), sub, Choice(value))
            }
            SettingRow::ProfileRename => (tr!("Rename current profile").into(), tr!("YASAFF restarts"), Action(tr!("Edit…"))),
            SettingRow::ProfileNew => (tr!("Create new profile").into(), tr!("An empty profile; YASAFF restarts into it"), Action(tr!("Create"))),
            SettingRow::ProfileExport => (tr!("Export current profile…").into(), tr!("Library, settings, themes, images and apps in one .zip (store API keys are left out)"), Action(tr!("Export"))),
            SettingRow::ProfileImport => (tr!("Import profile…").into(), tr!("An exported .zip; added as a new profile, renamed if the name is taken"), Action(tr!("Import"))),
            SettingRow::ProfileReset => (tr!("Reset current profile").into(), tr!("Deletes this profile's library, settings, themes and apps"), Action(tr!("Reset…"))),
            SettingRow::AppReset => (tr!("Reset application").into(), tr!("Deletes every profile and all data, as on a fresh install"), Action(tr!("Reset…"))),
            SettingRow::AppStoreAdd => (tr!("Add another store…").into(), tr!("Each store has its own address and API key"), Action(tr!("Add"))),
            SettingRow::AppStoreRemove => (tr!("Remove this store").into(), tr!("Installed apps and themes stay"), Action(tr!("Remove…"))),
            SettingRow::AboutApp => (
                "YASAFF".into(),
                tr!("A console-style game launcher for PC, made to be used with a controller. Windows and Linux."),
                RowValue::Summary(tr!("Version {version}", version = env!("CARGO_PKG_VERSION"))),
            ),
            SettingRow::AddonBar(i) => {
                let Some(addon) = self.addons.installed.get(i) else { return (String::new(), "", Action("")) };
                let label = match addon.min_height() {
                    Some(min) => tr!("{name} · needs a bar at least {px} px high", name = addon.name, px = format!("{min:.0}")),
                    None => addon.name.clone(),
                };
                let place = match AddonHost::placement(&self.settings, &addon.id) {
                    Some(slot) => slot.label(),
                    None => tr!("Off"),
                };
                (label, tr!("Bar widget · Left/Right picks the bar it shows in"), Choice(place.into()))
            }
            SettingRow::AddonRemove(i) => {
                let name = self.addons.installed.get(i).map_or("", |m| m.name.as_str());
                (tr!("Uninstall {name}", name = name), tr!("Its files are deleted; its saved data is kept"), Action(tr!("Uninstall…")))
            }
            SettingRow::AddonImport => (
                tr!("Import addon from file…").into(),
                tr!("A .zip with yasaffConfig.json; the Store's Addons tab has more"),
                Action(tr!("Choose…")),
            ),
            SettingRow::AboutSource => (tr!("Source code").into(), "github.com/naisyrkdev/YASAFF", Action(tr!("Open"))),
            SettingRow::AboutServer => (
                tr!("YASAFF Store server").into(),
                tr!("YasaffStuffServer: the website apps and themes come from"),
                Action(tr!("Open")),
            ),
            SettingRow::VirtualMouse if self.global_mouse.connected() => (
                tr!("Enable virtual mouse on right analog stick").into(),
                tr!("Yasaff Global Virtual Mouse is running and moves the mouse: right stick, RT left-click, LT right-click. Quit it to use this one."),
                RowValue::Summary(tr!("Handled by Global Virtual Mouse").into()),
            ),
            SettingRow::VirtualMouse => (
                tr!("Enable virtual mouse on right analog stick").into(),
                tr!("Right stick moves the cursor; RT or R3 clicks, LT right-clicks"),
                Toggle(self.settings.mouse.virtual_mouse),
            ),
            SettingRow::ControllerSwap => (
                tr!("Swap {a} with {b} action", a = button_token(PadButton::South), b = button_token(PadButton::East)),
                tr!("For those used to Nintendo Switch buttons; hidden while buttons are remapped"),
                Toggle(self.settings.controls.swap_ab),
            ),
            SettingRow::ControllerButton(button) => {
                let control = self.input.bindings().get(button);
                let sub = if self.global_mouse.connected() && GLOBAL_MOUSE_BUTTONS.contains(&button) {
                    tr!("Used by Yasaff Global Virtual Mouse while it runs")
                } else if control == button.default_control() {
                    ""
                } else {
                    tr!("Changed; left/right picks what it does")
                };
                (button_token(button), sub, Choice(control.label().into()))
            }
            SettingRow::SpecialPlace(special) => {
                let place = match self.special_bar(special) {
                    Some(slot) => slot.label(),
                    None => tr!("Library tile"),
                };
                match self.special_button(special) {
                    Some(button) => (
                        format!("{} · {}", special.name(), button_token(button)),
                        tr!("Left/Right picks where it shows: a library tile, or the top or bottom bar"),
                        Choice(place.into()),
                    ),
                    None => (special.name().into(), tr!("Assign it a controller button above to place it in a bar"), Choice(place.into())),
                }
            }
            SettingRow::ControllerReset => (
                tr!("Reset to default bindings").into(),
                tr!("Also brings back the A / B swap option"),
                Action(tr!("Reset")),
            ),
            SettingRow::MouseSpeed => (tr!("Virtual mouse speed").into(), tr!("At full tilt"), Choice(format!("{:.0}", self.settings.mouse.speed))),
            SettingRow::MouseCursor => (
                tr!("Import cursor…").into(),
                tr!("PNG with transparency works best; its top-left corner is the tip"),
                if self.settings.mouse.cursor.is_empty() { Action(tr!("Import")) } else { RowValue::Summary(tr!("Custom").into()) },
            ),
            SettingRow::MouseCursorClear => (tr!("Use the default cursor").into(), tr!("Removes the imported cursor"), Action(tr!("Reset"))),
            SettingRow::MouseCursorSize => (
                tr!("Cursor size").into(),
                tr!("For the imported cursor and the virtual mouse's"),
                Choice(format!("{:.0} px", self.settings.mouse.cursor_size)),
            ),
            SettingRow::FontTitle => {
                let title = &self.settings.fonts.title;
                let value = if title.is_empty() { tr!("Default").to_owned() } else { fonts::display_name(title).to_owned() };
                let sub = if self.fonts.is_empty() { tr!("Import a font below to change it") } else { tr!("Left/Right switches between imported fonts") };
                (tr!("Game title font").into(), sub, Choice(value))
            }
            SettingRow::FontImport => (tr!("Import fonts…").into(), tr!("TrueType or OpenType files (.ttf, .otf, .ttc)"), Action(tr!("Import"))),
            SettingRow::FontManage => (tr!("Manage fonts…").into(), tr!("See the imported fonts and delete them"), RowValue::Summary(tr!("{count} installed", count = self.fonts.len()))),
            SettingRow::GameTitleFont => {
                let default = match self.settings.fonts.title.as_str() {
                    "" => tr!("built-in").to_owned(),
                    title => fonts::display_name(title).to_owned(),
                };
                let value = match game.and_then(|g| g.title_font.as_deref()).filter(|f| self.fonts.iter().any(|l| l == f)) {
                    Some(file) => fonts::display_name(file).to_owned(),
                    None => tr!("Default ({font})", font = default),
                };
                let sub = if self.fonts.is_empty() { tr!("Import fonts in Settings to choose one") } else { tr!("Font of this game's name") };
                (tr!("Title font").into(), sub, Choice(value))
            }
            SettingRow::Theme => {
                let sub = if self.theme().is_builtin() { tr!("Built-in; it can't be deleted") } else { tr!("Left/Right switches between themes") };
                (tr!("Theme").into(), sub, Choice(theme.name.clone()))
            }
            SettingRow::ThemeRename => (tr!("Rename theme").into(), "", Action(tr!("Edit…"))),
            SettingRow::ThemeNew => (tr!("New theme").into(), tr!("Starts as a copy of the current theme"), Action(tr!("Create"))),
            SettingRow::ThemeExport => (tr!("Export theme…").into(), tr!("Copies the theme folder, images included, to a folder you pick"), Action(tr!("Export"))),
            SettingRow::ThemeImport => (tr!("Import theme…").into(), tr!("Select an exported theme folder (with themeSettings.json)"), Action(tr!("Import"))),
            SettingRow::ThemeDelete => (tr!("Delete theme").into(), "", Action(tr!("Delete…"))),
            SettingRow::ViewKind if theme.kind == ThemeKind::Bubbles => {
                (tr!("Layout").into(), tr!("Not available in this pre-defined theme"), Choice(theme.kind.label().into()))
            }
            SettingRow::ViewKind => (
                tr!("Layout").into(),
                tr!("Each layout keeps its own setup; switching back restores it"),
                Choice(theme.view.label().into()),
            ),
            SettingRow::TileStyle => {
                let sub = if theme.kind == ThemeKind::Bubbles { tr!("Not available in this pre-defined theme") } else { tr!("How games are drawn, in either layout") };
                (tr!("Tile style").into(), sub, Choice(theme.tile_style.label().into()))
            }
            SettingRow::BubbleSize => (
                tr!("Bubble size").into(),
                tr!("At 1080p; shrinks when needed to keep every game on screen"),
                px(theme.bubbles.size),
            ),
            SettingRow::BubbleVariance => (
                tr!("Random size difference").into(),
                tr!("Each bubble is up to this much smaller or bigger"),
                Choice(format!("± {:.0} px", theme.bubbles.size_variance)),
            ),
            SettingRow::BubbleFill => (
                tr!("Fill the screen with bubbles").into(),
                tr!("Empty bubbles fill the space around your games"),
                Toggle(theme.bubbles.fill_screen),
            ),
            SettingRow::BubbleShuffle => (tr!("Randomize bubbles").into(), tr!("Picks new random sizes and places"), Action(tr!("Shuffle"))),
            SettingRow::BubbleVideo => (
                tr!("Play video on selected game").into(),
                tr!("Set a video in the game's Options; needs ffmpeg installed"),
                Toggle(theme.bubbles.play_video),
            ),
            SettingRow::BubbleSound => (
                tr!("Sound when swapping bubbles").into(),
                tr!("A soft bloop when moving between bubbles"),
                Toggle(theme.bubbles.sound),
            ),
            SettingRow::BubbleCustomSound => (
                tr!("Use custom sound when swapping bubbles").into(),
                tr!("Plays your own sound file instead of the bloop"),
                Toggle(theme.bubbles.custom_sound),
            ),
            SettingRow::SwapSound => (
                tr!("Sound when swapping game").into(),
                tr!("A soft tick when moving between games"),
                Toggle(theme.swap.sound),
            ),
            SettingRow::SwapCustomSound => (
                tr!("Use custom sound when swapping game").into(),
                tr!("Plays your own sound file instead of the tick"),
                Toggle(theme.swap.custom_sound),
            ),
            SettingRow::BubbleSoundFile | SettingRow::SwapSoundFile => {
                let sub = if self.theme().sound_file(row == SettingRow::BubbleSoundFile).is_some() {
                    tr!("Copied into the theme's folder; WAV, MP3, OGG or FLAC")
                } else {
                    tr!("No sound selected; WAV, MP3, OGG or FLAC")
                };
                (tr!("Custom sound").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::BoxCase => {
                let label = if theme.tile_style == TileStyle::Cover { tr!("Cover shape") } else { tr!("Box shape") };
                (label.into(), tr!("Select box size"), Choice(boxes.case.label().into()))
            }
            SettingRow::BoxWidth | SettingRow::BoxHeight | SettingRow::BoxDepth => {
                let (label, sub, i) = match row {
                    SettingRow::BoxWidth => (tr!("Width"), tr!("50–400 mm; Left/Right steps, Enter to type a value"), 0),
                    SettingRow::BoxHeight => (tr!("Height"), tr!("50–400 mm; Left/Right steps, Enter to type a value"), 1),
                    _ => (tr!("Depth"), tr!("5–100 mm; Left/Right steps, Enter to type a value"), 2),
                };
                (label.into(), sub, Choice(format!("{:.0} mm", boxes.size_mm()[i])))
            }
            SettingRow::BoxSpineLabel => (tr!("Spine label").into(), tr!("On spines without a spine image"), Choice(boxes.spine_label.label().into())),
            SettingRow::BoxSpineColor => (tr!("Spine color").into(), tr!("Enter to type a hex value"), RowValue::Color(boxes.spine_color)),
            SettingRow::BoxSpineText => (tr!("Spine text color").into(), tr!("Enter to type a hex value"), RowValue::Color(boxes.spine_text_color)),
            SettingRow::BoxTopColor => (tr!("Top color").into(), tr!("Enter to type a hex value"), RowValue::Color(boxes.top_color)),
            SettingRow::GameCover => {
                let sub = if has(|g| g.asset(AssetKind::Cover).is_some()) { tr!("The front of the box") } else { tr!("None; the name's initials are shown") };
                (tr!("Cover image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameSpine => {
                let sub = if has(|g| g.asset(AssetKind::Spine).is_some()) {
                    tr!("Runs down the spine; a wide image is turned to fit")
                } else {
                    tr!("None; uses the spine color and label")
                };
                (tr!("Spine image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameAdjust(kind) => {
                let p = game.map(|g| g.placement(kind)).unwrap_or_default();
                let value = format!(
                    "{:.0} % · {} · {}",
                    p.zoom * 100.0,
                    position_label(p.x, tr!("Left"), tr!("Right")),
                    position_label(p.y, tr!("Top"), tr!("Bottom"))
                );
                (tr!("Size and position…").into(), tr!("Opens a before / after preview; nothing changes until you accept"), RowValue::Summary(value))
            }
            SettingRow::GameAdjustReset(_) => (tr!("Reset size and position").into(), "", Action(tr!("Reset"))),
            SettingRow::GameSpineClear => (tr!("Remove spine image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameTop => {
                let sub = if has(|g| g.asset(AssetKind::Top).is_some()) { tr!("Lies across the top of the box") } else { tr!("None; uses the top color") };
                (tr!("Top image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameTopClear => (tr!("Remove top image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameBack => {
                let sub = if has(|g| g.asset(AssetKind::Back).is_some()) { tr!("Seen while a 3D model box turns") } else { tr!("None; uses the spine color") };
                (tr!("Back cover image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameBackClear => (tr!("Remove back cover image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameSide => {
                let sub = if has(|g| g.asset(AssetKind::Side).is_some()) {
                    tr!("The edge opposite the spine; seen while a 3D model box turns")
                } else {
                    tr!("None; uses the top color")
                };
                (tr!("Right side image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameSideClear => (tr!("Remove right side image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameBottom => {
                let sub = if has(|g| g.asset(AssetKind::Bottom).is_some()) { tr!("Under the box; seen while editing it") } else { tr!("None; uses the top color") };
                (tr!("Bottom image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameBottomClear => (tr!("Remove bottom image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameBooklet => {
                let sub = if has(|g| g.asset(AssetKind::Booklet).is_some()) {
                    tr!("Inside the cover of a 3D opening case")
                } else {
                    tr!("None; the booklet shows the cover")
                };
                (tr!("Instruction booklet image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameBookletClear => (tr!("Remove instruction booklet image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameModel => {
                let sub = if has(|g| g.asset(AssetKind::Model).is_some()) {
                    tr!("STL, OBJ, PLY, glTF / GLB or 3MF; big models are simplified")
                } else {
                    tr!("None; the name is shown in 3D letters")
                };
                (tr!("3D model").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameModelUp => (
                tr!("Model up direction").into(),
                tr!("Models from CAD or 3D printing often need \"Front side up\""),
                Choice(game.map(|g| g.model_up).unwrap_or_default().label().into()),
            ),
            SettingRow::GameModelClear => (tr!("Remove 3D model").into(), tr!("Shows the name in 3D letters again"), Action(tr!("Remove"))),
            SettingRow::GameVideo => {
                let sub = if has(|g| g.asset(AssetKind::Video).is_some()) { tr!("Copied next to the game's images") } else { tr!("None; the cover is shown") };
                (tr!("Preview video").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameVideoClear => (tr!("Remove preview video").into(), "", Action(tr!("Remove"))),
            SettingRow::GameObjectColor => (
                tr!("Object color").into(),
                tr!("For the letters and models without colors; empty uses the theme's"),
                game_color(o.object_color, theme.objects.color),
            ),
            SettingRow::DiscKind => (
                tr!("Disc type").into(),
                tr!("All are 120 mm; the shiny data side differs"),
                Choice(theme.discs.kind.label().into()),
            ),
            SettingRow::DiscAngle => {
                let sub = if theme.discs.lying_flat() {
                    tr!("Set by the \"Flat, like in a drive\" spin")
                } else {
                    tr!("How far discs are tipped back; 0° faces you flat")
                };
                (tr!("Angle").into(), sub, Choice(format!("{:.0}°", theme.discs.shown_angle())))
            }
            SettingRow::DiscSpin => (tr!("Spinning").into(), tr!("The selected disc spins"), Toggle(theme.discs.spin)),
            SettingRow::DiscSpinDirection => (
                tr!("Spin").into(),
                tr!("Flipping over also shows the data side; flat lays the disc down"),
                Choice(theme.discs.spin_direction.label().into()),
            ),
            SettingRow::DiscSpinSpeed => (
                tr!("Spin speed").into(),
                tr!("Turns per minute"),
                Choice(format!("{:.0}", theme.discs.spin_speed)),
            ),
            SettingRow::ObjectColor => (
                tr!("Object color").into(),
                tr!("For the letters and models without colors; Enter to type a hex value"),
                RowValue::Color(theme.objects.color),
            ),
            SettingRow::GameSpineLabel => {
                let value = match o.spine_label {
                    Some(label) => label.label().to_owned(),
                    None => tr!("Default ({value})", value = boxes.spine_label.label()),
                };
                (tr!("Spine label").into(), "", Choice(value))
            }
            SettingRow::GameSpineColor => (tr!("Spine color").into(), game_color_sub, game_color(o.spine_color, boxes.spine_color)),
            SettingRow::GameSpineText => (tr!("Spine text color").into(), game_color_sub, game_color(o.spine_text_color, boxes.spine_text_color)),
            SettingRow::GameTopColor => (tr!("Top color").into(), game_color_sub, game_color(o.top_color, boxes.top_color)),
            SettingRow::TileRows => (
                tr!("Rows").into(),
                tr!("Up to 4; tiles shrink if they don't fit the screen"),
                Choice(tiles.rows.to_string()),
            ),
            SettingRow::TileSize => (tr!("Tile size").into(), tr!("At 1080p; scales with the window"), px(tiles.size)),
            SettingRow::TileSpacing => (tr!("Space between tiles").into(), tr!("At 1080p; scales with the window"), px(tiles.spacing)),
            SettingRow::EnlargeSelected => (tr!("Enlarge the selected tile").into(), "", Toggle(tiles.enlarge_selected)),
            SettingRow::SelectedScale => (
                tr!("Selected tile size").into(),
                tr!("Relative to the other tiles"),
                Choice(format!("{:.0} %", tiles.selected_scale * 100.0)),
            ),
            SettingRow::TileBar => {
                let sub = if tiles.rows > 1 { tr!("Only for a single row; set Rows to 1 to change it") } else { tr!("Only for a single row") };
                (tr!("Tile row position").into(), sub, Choice(tiles.bar_position.label().into()))
            }
            SettingRow::GridColumns => (tr!("Columns").into(), tr!("Tiles grow to fill the page"), Choice(grid.columns.to_string())),
            SettingRow::GridRows => (tr!("Rows per page").into(), tr!("More games continue on the next page"), Choice(grid.rows.to_string())),
            SettingRow::GridSpacing => (tr!("Space between tiles").into(), tr!("At 1080p; scales with the window"), px(grid.spacing)),
            SettingRow::GridEnlarge => (tr!("Enlarge the selected tile").into(), "", Toggle(grid.enlarge_selected)),
            SettingRow::GridAlign => (tr!("Grid position").into(), tr!("When the grid is narrower than the screen"), Choice(grid.align.label().into())),
            SettingRow::GameArt => (
                tr!("Use each game's background art").into(),
                tr!("Changes with the selected game; off keeps the theme background"),
                Toggle(bg.game_art),
            ),
            SettingRow::BackgroundKind => {
                let sub = if bg.game_art { tr!("Shown when the selected game has no background art") } else { tr!("Always shown") };
                (tr!("Background").into(), sub, Choice(bg.kind.label().into()))
            }
            SettingRow::BackgroundColor => {
                let sub = if bg.kind == BackgroundKind::Image { tr!("Around and behind the image") } else { tr!("Enter to type a hex value") };
                (tr!("Color").into(), sub, RowValue::Color(bg.color))
            }
            SettingRow::GradientFrom => (tr!("Start color").into(), tr!("Enter to type a hex value"), RowValue::Color(bg.gradient_from)),
            SettingRow::GradientTo => (tr!("End color").into(), tr!("Enter to type a hex value"), RowValue::Color(bg.gradient_to)),
            SettingRow::GradientDirection => (tr!("Direction").into(), "", Choice(bg.gradient_direction.label().into())),
            SettingRow::BackgroundImage => {
                let sub = if self.theme().background_image().is_some() { tr!("Copied into the theme's images folder") } else { tr!("No image selected") };
                (tr!("Image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::ImageFit => (tr!("Image layout").into(), "", Choice(bg.image_fit.label().into())),
            SettingRow::Bar(slot, field) => {
                let bar = theme.bar(slot);
                let percent = |v: f32| Choice(format!("{:+.0} %", v * 100.0));
                match field {
                    BarRow::Enabled => {
                        let sub = match slot {
                            BarSlot::Top => tr!("Behind the library count and the clock"),
                            BarSlot::Bottom => tr!("Behind the button hints"),
                        };
                        (tr!("Show the bar").into(), sub, Toggle(bar.enabled))
                    }
                    BarRow::Height => match self.bar_minimum(slot) {
                        Some((min, addon)) => (
                            tr!("Height · {addon} needs at least {px} px", addon = addon, px = format!("{min:.0}")),
                            tr!("At 1080p; raised to what the addons in this bar need"),
                            px(bar.height(slot).max(min)),
                        ),
                        None => (tr!("Height").into(), tr!("At 1080p; the text inside stays centered"), px(bar.height(slot))),
                    },
                    BarRow::Kind => (tr!("Fill").into(), "", Choice(bar.kind.label().into())),
                    BarRow::Color => {
                        let sub = if bar.kind == BackgroundKind::Image {
                            tr!("Around and behind the image")
                        } else {
                            tr!("Enter to type a hex value; #rrggbbaa makes it see-through")
                        };
                        (tr!("Color").into(), sub, RowValue::Color(bar.color))
                    }
                    BarRow::GradientFrom => (tr!("Start color").into(), tr!("Enter to type a hex value"), RowValue::Color(bar.gradient_from)),
                    BarRow::GradientTo => (tr!("End color").into(), tr!("Enter to type a hex value"), RowValue::Color(bar.gradient_to)),
                    BarRow::GradientDirection => (tr!("Direction").into(), "", Choice(bar.gradient_direction.label().into())),
                    BarRow::Image => {
                        let sub = if self.theme().bar_image(slot).is_some() { tr!("Copied into the theme's images folder") } else { tr!("No image selected") };
                        (tr!("Image").into(), sub, Action(tr!("Choose…")))
                    }
                    BarRow::ImageZoom => (tr!("Image size").into(), tr!("At 100 % the image just covers the bar"), Choice(format!("{:.0} %", bar.image_zoom * 100.0))),
                    BarRow::ImageX => (tr!("Horizontal position").into(), tr!("Moves the image sideways"), percent(bar.image_x)),
                    BarRow::ImageY => (tr!("Vertical position").into(), tr!("Moves the image up and down"), percent(bar.image_y)),
                    BarRow::ImageRotation => (tr!("Rotation").into(), tr!("Turns the image in 5° steps"), Choice(format!("{:.0}°", bar.image_rotation))),
                    BarRow::ImageReset => (tr!("Reset image placement").into(), tr!("Back to 100 %, centered, not rotated"), Action(tr!("Reset"))),
                }
            }
            SettingRow::Button(button, field) => {
                let look = theme.buttons.get(button);
                let token = face_token(button);
                let optional = |color: Option<Color32>, default: Color32| match color {
                    Some(c) => RowValue::Color(c),
                    None => RowValue::DefaultColor(default),
                };
                let color_sub = tr!("Enter to type a hex value; empty uses the default");
                match field {
                    ButtonRow::Color => {
                        (tr!("{button} color", button = token), color_sub, optional(look.color, glyph_color(button.glyph())))
                    }
                    ButtonRow::TextColor => (tr!("{button} letter color", button = token), color_sub, optional(look.text_color, Color32::WHITE)),
                    ButtonRow::Icon => {
                        let sub = if self.theme().button_icon(button).is_some() {
                            tr!("Copied into the theme's images folder")
                        } else {
                            tr!("A .png image drawn instead of the colored button")
                        };
                        (tr!("{button} icon", button = token), sub, Action(tr!("Choose…")))
                    }
                    ButtonRow::IconClear => (tr!("Remove {button} icon", button = token), tr!("Back to the colored button"), Action(tr!("Remove"))),
                }
            }
            SettingRow::AppStoreUrl => {
                let url = self.settings.app_store().url.trim();
                let value = if url.is_empty() { tr!("Not set").to_owned() } else { url.to_owned() };
                (tr!("Store address").into(), tr!("From the store's API keys page, e.g. https://store.example.com/api/v1"), RowValue::Summary(value))
            }
            SettingRow::AppStoreKey => (tr!("API key").into(), tr!("Paste a key created on the store's API keys page"), RowValue::Summary(self.masked_api_key())),
            SettingRow::AppStoreTest => (tr!("Test connection").into(), tr!("The Store tile lists the store's apps once this works"), Action(tr!("Test"))),
            SettingRow::StoreEnabled(st) => (
                tr!("Launch {store} games through {store}", store = st.short_name()),
                tr!("Its window stays hidden; games start straight from YASAFF"),
                Toggle(self.settings.store(st).enabled),
            ),
            SettingRow::StoreBackground(st) => (
                tr!("Start {store} in the background with YASAFF", store = st.short_name()),
                tr!("Minimized to the tray, below YASAFF, ready for fast launches"),
                Toggle(self.settings.store(st).start_in_background),
            ),
            SettingRow::StoreCommand(st) => {
                let custom = !self.settings.store(st).command.trim().is_empty();
                (tr!("{store} command", store = st.short_name()), if custom { tr!("Custom") } else { tr!("Auto-detected") }, Action(tr!("Edit…")))
            }
            SettingRow::DiscOnInsert => (
                tr!("When a disc is inserted").into(),
                tr!("Asks anyway while YASAFF is in the background"),
                Choice(self.settings.discs.on_insert.label().into()),
            ),
            SettingRow::DiscScanAtStart => (
                tr!("Check the drive when YASAFF starts").into(),
                tr!("A disc left in the drive starts its game, like a console"),
                Toggle(self.settings.discs.scan_at_start),
            ),
            SettingRow::DiscRemovable => (
                tr!("Enable USB drive (USB stick) and SD card support").into(),
                tr!("This option will make those devices behave like a disc"),
                Toggle(self.settings.discs.removable_drives),
            ),
        }
    }

    pub(super) fn draw_settings(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::Settings { cursor, page, in_rows } = self.mode else { return };
        let rows = self.setting_rows(page);
        let sections = self.setting_sections(&rows);
        let title = match (page, self.selected_game()) {
            (Page::GameBox, Some(game)) => tr!("Appearance · {name}", name = game.name),
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
            nav_painter.text(pos2(r.left() + 24.0 * s, r.center().y), Align2::LEFT_CENTER, section.title, FontId::proportional(20.0 * s), color);
            if resp.clicked() {
                chosen_section = Some(i);
            }
        }

        let section = &sections[current];
        let column = Rect::from_min_max(pos2(nav.right() + 20.0 * s, body_top), pos2(panel.right() - 20.0 * s, nav.bottom()));
        painter.text(pos2(column.left() + 20.0 * s, column.top()), Align2::LEFT_TOP, section.title, FontId::proportional(24.0 * s), ACCENT);
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

        let skin = |glyph: &str| self.button_skin(glyph);
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
            let x = r.left() + 30.0 * s;
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
}

impl App {
    pub(super) fn handle_settings(&mut self, action: Action) {
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
                Page::GameBox => Mode::Options { cursor: self.menu_index(MenuItem::BoxAppearance) },
            }
        }
    }
}
