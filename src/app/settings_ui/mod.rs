//! The Settings screen: its pages and rows, what each row shows, and what Left/Right and Confirm do on it.
//!
//! - `rows`: which rows each page has, how they group into sections, and their headings.
//! - `actions`: what Confirm (`activate_setting`) and Left/Right (`adjust_setting`) do on a row.
//! - `content`: the label, description and value each row shows.
//! - `screen`: drawing the screen and moving around it.

mod actions;
mod content;
mod rows;
mod screen;

use super::*;
pub(super) use content::slot_label;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum SettingRow {
    Fullscreen,
    Autostart,
    Language,
    OrderLastPlayed,
    OrderStatic,
    GalleryKeepAll,
    ProfileActive,
    ProfileRename,
    ProfileNew,
    ProfileExport,
    ProfileImport,
    ProfileReset,
    AppReset,
    AppStoreActive,
    AppStoreUrl,
    AppStoreTest,
    AppStoreAdd,
    AppStoreRemove,
    VirtualMouse,
    MouseSpeed,
    MouseCursor,
    MouseCursorClear,
    MouseCursorSize,
    ControllerPrompts,
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
    AxisWidth,
    AxisHeight,
    AxisBorder,
    AxisFont,
    AxisFontSize,
    AxisDynamicFont,
    AxisTextColor,
    AxisGlowColor,
    AxisGlowIntensity,
    AxisTitleInfo,
    AxisImages,
    /// A category's image, on the category images page.
    AxisImage(AxisSlot),
    AxisImageAdjust(AxisSlot),
    AxisImageClear(AxisSlot),
    CategoryAdd,
    /// A category in Settings, by its index in the library's list.
    Category(usize),
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
    CrtWidth,
    CrtHeight,
    CrtVideo,
    CrtIdleVideo,
    CrtFilter,
    GameArt,
    BackgroundKind,
    BackgroundColor,
    GradientFrom,
    GradientTo,
    GradientDirection,
    BackgroundImage,
    ImageFit,
    Bar(BarSlot, BarRow),
    ButtonStyle,
    ButtonPick,
    Button(PadButton, ButtonRow),
    TileIcon(Special),
    TileIconClear(Special),
    StoreEnabled(Store),
    StoreBackground(Store),
    StoreCommand(Store),
    DiscOnInsert,
    DiscScanAtStart,
    DiscRemovable,
    DiscKeepSaves,
    DeviceTarget,
    DeviceCreateFile,
    DeviceBurn,
    DeviceInsertConfig,
    DeviceCreateCatalog,
    DeviceBackupNow,
    DeviceKeepSaves,
    DeviceRecovery,
    DeviceOpenSaveFolder,
    DeviceSetSaveFolder,
    DeviceSavePath(usize),
    DeviceSaveFolder,
    DeviceAddFiles,
    DeviceAddFolders,
    /// The cover a tile style shows.
    StyleCover(TileStyle),
    StyleAdjust(TileStyle),
    StyleAdjustReset(TileStyle),
    GameBackground,
    GameBackgroundClear,
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
    GameIdleVideo,
    GameIdleVideoClear,
    GameObjectColor,
    GameSpineLabel,
    GameSpineColor,
    GameSpineText,
    GameTopColor,
    GameTitleFont,
    GameAxisImage,
    GameAxisImageClear,
    GameMainCategory,
    /// Whether the game is in a category, by the category's index in the library's list.
    GameCategory(usize),
    GameNoCategories,
    /// An image in the game's gallery, by its index there.
    GalleryImage(usize),
    GalleryEmpty,
    AddonManager,
    AboutApp,
    AboutSource,
    AboutServer,
}

/// A column of the Double axis theme, as the category images page names it.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum AxisSlot {
    Category(usize),
    Uncategorized,
    Main,
}

pub(super) fn axis_slot_column(library: &Library, slot: AxisSlot) -> Option<theme::AxisColumn<'_>> {
    match slot {
        AxisSlot::Category(i) => library.categories.get(i).map(|name| theme::AxisColumn::Category(name)),
        AxisSlot::Uncategorized => Some(theme::AxisColumn::Uncategorized),
        AxisSlot::Main => Some(theme::AxisColumn::Main),
    }
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
    Icon,
    IconClear,
    Shape,
    Color,
    TextColor,
}

pub(super) const SOURCE_URL: &str = "https://github.com/naisyrkdev/YASAFF";

pub(super) const SERVER_URL: &str = "https://github.com/naisyrkdev/YasaffStuffServer";

impl SettingRow {
    pub(super) fn page(self) -> Page {
        use SettingRow as R;
        match self {
            R::StyleCover(_)
            | R::StyleAdjust(_)
            | R::StyleAdjustReset(_)
            | R::GalleryImage(_)
            | R::GalleryEmpty
            | R::GameBackground
            | R::GameBackgroundClear
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
            | R::GameIdleVideo
            | R::GameIdleVideoClear
            | R::GameObjectColor
            | R::GameSpineLabel
            | R::GameSpineColor
            | R::GameSpineText
            | R::GameTopColor
            | R::GameTitleFont
            | R::GameAxisImage
            | R::GameAxisImageClear => Page::GameBox,
            R::GameMainCategory | R::GameCategory(_) | R::GameNoCategories => Page::GameDetails,
            R::AxisImage(_) | R::AxisImageAdjust(_) | R::AxisImageClear(_) => Page::CategoryImages,
            R::DeviceTarget
            | R::DeviceCreateFile
            | R::DeviceBurn
            | R::DeviceInsertConfig
            | R::DeviceCreateCatalog
            | R::DeviceBackupNow
            | R::DeviceKeepSaves
            | R::DeviceRecovery
            | R::DeviceOpenSaveFolder
            | R::DeviceSetSaveFolder
            | R::DeviceSavePath(_)
            | R::DeviceSaveFolder
            | R::DeviceAddFiles
            | R::DeviceAddFolders => Page::GameDevice,
            _ => Page::App,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Page {
    App,
    GameBox,
    /// A game's Physical Devices page: its launch file, and backing its saves up to a device.
    GameDevice,
    /// A game's categories.
    GameDetails,
    /// The Double axis theme's category images.
    CategoryImages,
}

impl Page {
    /// Whether the page changes the selected game.
    pub(super) fn is_game(self) -> bool {
        matches!(self, Page::GameBox | Page::GameDevice | Page::GameDetails)
    }
}

pub(super) struct SettingSection {
    pub(super) title: &'static str,
    pub(super) status: Option<(String, Color32)>,
    /// The way the game is shown now, on its Appearance page; marked with a green dot.
    pub(super) shown: bool,
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
