
mod active_game;
mod keyboard;
mod store_screen;

use crate::addons::{self, AddonHost};
use crate::apps::{self, AppEntry};
use crate::autostart;
use crate::box3d;
use crate::case3d;
use crate::disc3d;
use crate::fonts;
use crate::i18n;
use crate::initial;
use crate::global_mouse::GlobalMouse;
use crate::input::{Action, Control, GLOBAL_MOUSE_BUTTONS, Input, PadButton};
use crate::launcher;
use crate::library::{AssetKind, CoverPlacement, Game, Library};
use crate::metadata::{self, Fetcher};
use crate::object3d::{self, Objects};
use crate::profiles;
use crate::scan;
use crate::sound::Sounds;
use crate::settings::{DiscAction, Language, MouseSettings, Settings};
use crate::stores::Store;
use crate::video::{self, VideoPlayer};
use crate::theme::{
    self, BackgroundKind, BarPosition, BarSlot, BarStyle, BoxCase, BubbleSettings, DiscKind, DiscSettings, DiscSpin, FaceButton, GradientDirection, GridAlign, GridSettings,
    ImageFit, SpineLabel, Theme, ThemeKind, TileSettings, TileStyle, ViewKind,
};
use egui::{
    Align2, Color32, Context, FontId, Id, Mesh, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TextureHandle, TextureOptions,
    TextureId, Ui, Vec2, ViewportCommand, pos2, vec2,
};
use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use crate::physical::burn::{self, BurnJob, Burner};
use active_game::RunningGame;
use crate::watch::GameWatch;
use store_screen::{RunningApp, StoreState};
use crate::physical::{DiscEvent, DiscManifest, Resolution, Volume, VolumeKind, WatchOptions, Watcher};

const ACCENT: Color32 = Color32::from_rgb(0x2f, 0x7d, 0xf6);
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp"];
const BG_FADE: Duration = Duration::from_millis(350);
const TOAST_TIME: Duration = Duration::from_secs(4);
const LAUNCH_MIN: Duration = Duration::from_millis(1500);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(120);
const FOCUS_GUARD: Duration = Duration::from_secs(8);
const SPIN_TIME: Duration = Duration::from_secs(12);
const DISC_FOCUS_GRACE: Duration = Duration::from_secs(5);
const BURN_QUIET: Duration = Duration::from_secs(20);
const BURNER_REFRESH: Duration = Duration::from_secs(1);
const VIDEO_DELAY: Duration = Duration::from_millis(700);

#[derive(Clone, Copy, PartialEq)]
enum MenuItem {
    Start,
    CloseApp,
    Update,
    SetCover,
    SetBackground,
    FetchArtwork,
    BoxAppearance,
    Rename,
    SwapBubble,
    EditCommand,
    SaveFolder,
    OpenSaveFolder,
    BackupSaves,
    CreateDisc,
    BurnDisc,
    Remove,
}

const MENU: [(MenuItem, &str); 12] = [
    (MenuItem::Start, tr_key!("Start")),
    (MenuItem::BoxAppearance, tr_key!("Tile and box appearance…")),
    (MenuItem::SetCover, tr_key!("Set cover image")),
    (MenuItem::SetBackground, tr_key!("Set background image")),
    (MenuItem::FetchArtwork, tr_key!("Fetch artwork")),
    (MenuItem::Rename, tr_key!("Rename")),
    (MenuItem::SwapBubble, tr_key!("Swap with other bubble")),
    (MenuItem::EditCommand, tr_key!("Edit launch command")),
    (MenuItem::SaveFolder, tr_key!("Set save folder…")),
    (MenuItem::CreateDisc, tr_key!("Create disc file…")),
    (MenuItem::BurnDisc, tr_key!("Burn to CD/DVD…")),
    (MenuItem::Remove, tr_key!("Remove from library")),
];

#[derive(Clone, Copy, PartialEq)]
enum Special {
    Store,
    Add,
    Settings,
}

impl Special {
    const ALL: [Special; 3] = [Special::Store, Special::Add, Special::Settings];
    const PLACEABLE: [Special; 2] = [Special::Store, Special::Add];

    fn action(self) -> Option<Action> {
        match self {
            Special::Store => Some(Action::OpenStore),
            Special::Add => Some(Action::AddGame),
            Special::Settings => None,
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Special::Store => "🏪",
            Special::Add => "+",
            Special::Settings => "⚙",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Special::Store => tr!("Store"),
            Special::Add => tr!("Add game"),
            Special::Settings => tr!("Settings"),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum AddSource {
    Scan,
    Installed,
    Browse,
    Command,
    ImportApp,
}

const ADD_MENU: [(AddSource, &str, &str); 5] = [
    (AddSource::Scan, tr_key!("Scan for games"), tr_key!("Finds installed Steam and Heroic games and game shortcuts")),
    (AddSource::Installed, tr_key!("Installed applications"), tr_key!("Games, desktop shortcuts (Heroic, Steam, Lutris…) and apps")),
    (AddSource::Browse, tr_key!("Browse for file…"), tr_key!("Executable, script, AppImage, .desktop or shortcut")),
    (AddSource::Command, tr_key!("Command or URL…"), tr_key!("e.g. steam://rungameid/570 or a full command line")),
    (AddSource::ImportApp, tr_key!("Import application from file…"), tr_key!("A YASAFF app or addon package: a .zip with yasaffConfig.json")),
];

#[derive(Clone, Copy, PartialEq)]
enum Start {
    Import,
    Blank,
    Demo,
}

const WELCOME: [(Start, &str, &str); 3] = [
    (Start::Import, tr_key!("Import games"), tr_key!("Pick from the Steam, Heroic and shortcut games installed on this computer")),
    (Start::Blank, tr_key!("Start blank"), tr_key!("An empty library; add games any time with Add game")),
    (Start::Demo, tr_key!("Start blank (with mock games)"), tr_key!("All customization images included")),
];

#[derive(Clone, Copy, PartialEq)]
enum PromptKind {
    Rename,
    AddCommand,
    EditCommand,
    StoreCommand(Store),
    ThemeName,
    AppStoreUrl,
    AppStoreKey,
    Color(SettingRow),
    BoxSize(SettingRow),
    ProfileName,
    ProfileNew,
}

#[derive(Clone, Copy, PartialEq)]
enum SettingRow {
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
enum BarRow {
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
enum ButtonRow {
    Color,
    TextColor,
    Icon,
    IconClear,
}

const SOURCE_URL: &str = "https://github.com/naisyrkdev/YASAFF";
const SERVER_URL: &str = "https://github.com/naisyrkdev/YasaffStuffServer";

impl SettingRow {
    fn page(self) -> Page {
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
enum Page {
    App,
    GameBox,
}

struct SettingSection {
    title: &'static str,
    status: Option<(String, Color32)>,
    rows: Range<usize>,
}

enum RowValue {
    Toggle(bool),
    Choice(String),
    Color(Color32),
    DefaultColor(Color32),
    Action(&'static str),
    Summary(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Removal {
    Game,
    Theme,
    AppStore,
    Addon(usize),
    Profile,
    Everything,
}

struct Launching {
    name: String,
    font: egui::FontFamily,
    cover: Option<PathBuf>,
    via: Option<Store>,
    store_app: bool,
    started: Instant,
    watch: GameWatch,
}

impl Drop for Launching {
    fn drop(&mut self) {
        if self.store_app {
            self.watch.stop();
        }
    }
}

enum Mode {
    Home,
    Welcome { cursor: usize },
    Options { cursor: usize },
    SwapBubble { from: usize, target: usize },
    AddMenu { cursor: usize },
    Picker(Box<Picker>),
    Scan(Box<ScanState>),
    ConfirmRemove { what: Removal, yes: bool },
    Prompt { kind: PromptKind, buffer: String },
    Settings { cursor: usize, page: Page, in_rows: bool },
    Launching(Launching),
    DiscChooser { name: String, candidates: Vec<usize>, cursor: usize },
    DiscNotFound { name: String },
    CoverEditor(CoverEditor),
    FontManager { cursor: usize, armed: bool },
    Burn(BurnDialog),
    Store { cursor: usize, scroll: usize },
    StoreTheme { index: usize, image: usize },
    ProfileImported { name: String, missing: Vec<String>, switch: bool },
}

struct InitialSetup {
    rx: mpsc::Receiver<initial::Step>,
    total: usize,
    done: usize,
}

enum ProfileJob {
    Exported(Result<PathBuf, String>),
    Imported(Result<profiles::Imported, String>),
}

struct BurnDialog {
    burners: Vec<Burner>,
    looked: Instant,
    cursor: usize,
    armed: bool,
}

struct Burning {
    game: String,
    drive: String,
    rx: mpsc::Receiver<BurnMsg>,
    progress: burn::Progress,
    result: Option<Result<(), String>>,
}

enum BurnMsg {
    Progress(burn::Progress),
    Done(Result<(), String>),
}

struct CoverEditor {
    kind: AssetKind,
    draft: CoverPlacement,
    cursor: usize,
    button: EditorButton,
}

#[derive(Clone, Copy, PartialEq)]
enum EditorButton {
    Reset,
    Cancel,
    Accept,
}

impl EditorButton {
    const ALL: [Self; 3] = [Self::Reset, Self::Cancel, Self::Accept];

    fn label(self) -> &'static str {
        match self {
            Self::Reset => tr!("Reset"),
            Self::Cancel => tr!("Cancel"),
            Self::Accept => tr!("Accept"),
        }
    }
}

#[derive(Clone, Copy)]
enum CoverSlider {
    Size,
    Horizontal,
    Vertical,
}

impl CoverSlider {
    const ALL: [Self; 3] = [Self::Size, Self::Horizontal, Self::Vertical];

    fn label(self) -> &'static str {
        match self {
            Self::Size => tr!("Size"),
            Self::Horizontal => tr!("Horizontal position"),
            Self::Vertical => tr!("Vertical position"),
        }
    }

    fn get(self, p: &CoverPlacement) -> f32 {
        let (min, max) = CoverPlacement::ZOOM_RANGE;
        match self {
            Self::Size => (p.zoom - min) / (max - min),
            Self::Horizontal => p.x,
            Self::Vertical => p.y,
        }
    }

    fn set(self, p: &mut CoverPlacement, t: f32) {
        let (min, max) = CoverPlacement::ZOOM_RANGE;
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Size => p.zoom = snap(min + (max - min) * t),
            Self::Horizontal => p.x = snap(t),
            Self::Vertical => p.y = snap(t),
        }
    }

    fn step(self, p: &mut CoverPlacement, delta: i32) {
        let d = 0.05 * delta as f32;
        match self {
            Self::Size => p.zoom = snap(step(p.zoom, d, CoverPlacement::ZOOM_RANGE)),
            Self::Horizontal => p.x = snap(step(p.x, d, (0.0, 1.0))),
            Self::Vertical => p.y = snap(step(p.y, d, (0.0, 1.0))),
        }
    }

    fn value(self, p: &CoverPlacement) -> String {
        match self {
            Self::Size => format!("{:.0} %", p.zoom * 100.0),
            Self::Horizontal => position_label(p.x, tr!("Left"), tr!("Right")),
            Self::Vertical => position_label(p.y, tr!("Top"), tr!("Bottom")),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Games,
    Desktop,
    All,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Games => tr!("Games"),
            Tab::Desktop => tr!("Desktop shortcuts"),
            Tab::All => tr!("All applications"),
        }
    }

    fn matches(self, app: &AppEntry) -> bool {
        match self {
            Tab::Games => app.is_game,
            Tab::Desktop => app.on_desktop,
            Tab::All => true,
        }
    }
}

struct Picker {
    apps: Vec<AppEntry>,
    tabs: Vec<Tab>,
    tab: usize,
    cursor: usize,
    scroll: usize,
    filter: String,
}

impl Picker {
    fn new() -> Self {
        let apps = apps::installed();
        let tabs = [Tab::Games, Tab::Desktop, Tab::All]
            .into_iter()
            .filter(|t| *t == Tab::All || apps.iter().any(|a| t.matches(a)))
            .collect();
        Picker { apps, tabs, tab: 0, cursor: 0, scroll: 0, filter: String::new() }
    }

    fn visible(&self) -> Vec<usize> {
        let tab = self.tabs[self.tab];
        let filter = self.filter.to_lowercase();
        (0..self.apps.len())
            .filter(|&i| tab.matches(&self.apps[i]) && self.apps[i].name.to_lowercase().contains(&filter))
            .collect()
    }
}

struct ScanState {
    pending: Option<mpsc::Receiver<(Vec<scan::Found>, usize)>>,
    found: Vec<scan::Found>,
    checked: Vec<bool>,
    in_library: usize,
    assets_rx: Option<mpsc::Receiver<(usize, scan::Assets)>>,
    assets: Vec<Option<FoundAssets>>,
    fetch_artwork: bool,
    first_run: bool,
    cursor: usize,
    scroll: usize,
}

struct FoundAssets {
    cover: bool,
    background: bool,
    thumbnail: Option<TextureHandle>,
}

impl ScanState {
    fn start(ctx: &Context, known: scan::Known, fetch_artwork: bool, first_run: bool) -> Self {
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

    fn fetch_row(&self) -> usize {
        self.found.len()
    }

    fn import_row(&self) -> usize {
        self.found.len() + 1
    }

    fn ticked(&self) -> usize {
        self.checked.iter().filter(|c| **c).count()
    }
}

#[derive(Clone, Copy)]
struct Hint(Action, &'static str);

const PAD_A: Color32 = Color32::from_rgb(0x5c, 0xb8, 0x3c);
const PAD_B: Color32 = Color32::from_rgb(0xd9, 0x3a, 0x3a);
const PAD_X: Color32 = Color32::from_rgb(0x2f, 0x7f, 0xe0);
const PAD_Y: Color32 = Color32::from_rgb(0xe8, 0xb9, 0x24);
const PAD_MENU: Color32 = Color32::from_gray(90);
const KEY_FILL: Color32 = Color32::from_rgba_premultiplied(40, 40, 40, 40);
const BUTTON_PRESETS: &[Color32] = &[PAD_A, PAD_B, PAD_X, PAD_Y, PAD_MENU, Color32::WHITE, Color32::BLACK];
const BUTTON_OPEN: char = '\u{E000}';
const BUTTON_CLOSE: char = '\u{E001}';

pub struct App {
    ctx: Context,
    library: Library,
    settings: Settings,
    themes: Vec<Theme>,
    theme: usize,
    selected: usize,
    input: Input,
    global_mouse: GlobalMouse,
    mode: Mode,
    toast: Option<(String, Instant)>,
    bubble_layout: RefCell<Option<BubbleLayout>>,
    bubble_empty: Option<usize>,
    sounds: Sounds,
    video: Option<VideoPlayer>,
    video_wanted: (Option<PathBuf>, Instant),
    video_failed: bool,
    fullscreen: bool,
    autostart: bool,
    bg_current: Option<PathBuf>,
    bg_previous: Option<PathBuf>,
    bg_changed: Instant,
    focus_guard_until: Option<Instant>,
    spin: Cell<(usize, Instant)>,
    objects: RefCell<Objects>,
    last_focused: Instant,
    discs: mpsc::Receiver<DiscEvent>,
    disc_removable: Arc<AtomicBool>,
    _disc_watcher: Watcher,
    artwork: Fetcher,
    fonts: Vec<String>,
    fonts_pending: Option<Vec<String>>,
    burning: Option<Burning>,
    discs_quiet_until: Option<Instant>,
    store: StoreState,
    addons: AddonHost,
    running: Vec<RunningApp>,
    game: Option<RunningGame>,
    save_dirs_found: Option<mpsc::Receiver<Vec<(String, PathBuf)>>>,
    backup: Option<(String, mpsc::Receiver<Result<PathBuf, String>>)>,
    keyboard: keyboard::Keyboard,
    profiles: Vec<String>,
    profile_pick: String,
    profile_job: Option<mpsc::Receiver<ProfileJob>>,
    setup: Option<InitialSetup>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, settings: Settings, fullscreen: bool, disc: Option<PathBuf>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let (library, warning) = Library::load();
        let (themes, theme_warnings) = Theme::load_all();

        let (disc_tx, discs) = mpsc::channel();
        let mut disc_warning = None;
        if let Some(path) = disc {
            match disc_from_path(&path) {
                Ok(event) => disc_tx.send(event).unwrap(),
                Err(e) => disc_warning = Some(format!("--disc {}: {e}", path.display())),
            }
        }
        let disc_removable = Arc::new(AtomicBool::new(settings.discs.removable_drives));
        let opts = WatchOptions {
            scan_existing: settings.discs.scan_at_start,
            removable: disc_removable.clone(),
            ..WatchOptions::default()
        };
        let mut input = Input::new();
        input.set_virtual_mouse(settings.mouse.virtual_mouse);
        input.set_bindings(settings.controls.bindings());
        let ctx = cc.egui_ctx.clone();
        let disc_watcher = Watcher::spawn(opts, move |event| {
            if disc_tx.send(event).is_ok() {
                ctx.request_repaint();
            }
        });
        let store = StoreState::new(cc.egui_ctx.clone(), &library);
        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
            library,
            settings,
            themes,
            theme: 0,
            selected: 0,
            input,
            global_mouse: GlobalMouse::spawn(cc.egui_ctx.clone()),
            mode: Mode::Home,
            toast: None,
            bubble_layout: RefCell::new(None),
            bubble_empty: None,
            sounds: Sounds::default(),
            video: None,
            video_wanted: (None, Instant::now()),
            video_failed: false,
            fullscreen,
            autostart: autostart::is_enabled(),
            bg_current: None,
            bg_previous: None,
            bg_changed: Instant::now(),
            focus_guard_until: None,
            spin: Cell::new((0, Instant::now())),
            objects: RefCell::new(Objects::new(cc.egui_ctx.clone())),
            last_focused: Instant::now(),
            discs,
            disc_removable,
            _disc_watcher: disc_watcher,
            artwork: Fetcher::new(cc.egui_ctx.clone()),
            fonts: Vec::new(),
            fonts_pending: None,
            burning: None,
            discs_quiet_until: None,
            keyboard: keyboard::Keyboard::default(),
            store,
            addons: AddonHost::new(cc.egui_ctx.clone()),
            running: Vec::new(),
            game: None,
            save_dirs_found: None,
            backup: None,
            profiles: profiles::list(),
            profile_pick: profiles::active(),
            profile_job: None,
            setup: None,
        };
        app.apply_fonts();
        if let Some(w) = warning {
            app.notify(tr!("Could not read library: {error}", error = w));
        }
        if let Some(w) = disc_warning {
            app.notify(w);
        }
        for w in theme_warnings {
            app.notify(w);
        }
        match app.themes.iter().position(|t| t.id == app.settings.theme) {
            Some(i) => app.theme = i,
            None => {
                app.settings.theme = app.themes[0].id.clone();
                app.save_settings();
            }
        }
        for store in Store::ALL {
            if app.settings.store(store).start_in_background {
                app.start_store_in_background(store);
            }
        }
        app.refresh_store();
        app.find_missing_save_dirs();
        if initial::wanted(&app.library) {
            app.mode = Mode::Welcome { cursor: 0 };
        }
        app
    }

    fn selected_game(&self) -> Option<&Game> {
        self.library.games.get(self.selected)
    }

    fn tile_count(&self) -> usize {
        self.library.games.len() + self.special_tiles().count()
    }

    fn special(&self, i: usize) -> Option<Special> {
        self.special_tiles().nth(i.checked_sub(self.library.games.len())?)
    }

    fn special_tiles(&self) -> impl Iterator<Item = Special> + '_ {
        Special::ALL.into_iter().filter(|s| self.special_bar(*s).is_none())
    }

    fn special_button(&self, special: Special) -> Option<PadButton> {
        self.input.bindings().button_for(special.action()?.control())
    }

    fn special_place(settings: &mut Settings, special: Special) -> Option<&mut Option<BarSlot>> {
        match special {
            Special::Store => Some(&mut settings.shortcuts.store),
            Special::Add => Some(&mut settings.shortcuts.add_game),
            Special::Settings => None,
        }
    }

    fn special_bar(&self, special: Special) -> Option<BarSlot> {
        self.special_button(special)?;
        match special {
            Special::Store => self.settings.shortcuts.store,
            Special::Add => self.settings.shortcuts.add_game,
            Special::Settings => None,
        }
    }

    fn bar_shortcuts(&self, slot: BarSlot) -> Vec<Hint> {
        Special::PLACEABLE
            .into_iter()
            .filter(|s| self.special_bar(*s) == Some(slot))
            .filter_map(|s| Some(Hint(s.action()?, s.name())))
            .collect()
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.tile_count() - 1);
    }

    fn notify(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        eprintln!("{msg}");
        self.toast = Some((msg, Instant::now()));
    }

    fn save(&mut self) {
        if let Err(e) = self.library.save() {
            self.notify(tr!("Could not save library: {error}", error = format!("{e:#}")));
        }
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.notify(tr!("Could not save settings: {error}", error = format!("{e:#}")));
        }
    }

    fn controls_changed(&mut self) {
        self.input.set_bindings(self.settings.controls.bindings());
        self.clamp_selection();
        self.save_settings();
    }

    fn global_mouse_status(&self) -> Option<(String, Color32)> {
        self.global_mouse.connected().then(|| (tr!("Yasaff Global Virtual Mouse integration mode: ON").to_owned(), PAD_A))
    }

    fn controller_warning(&self) -> (String, Color32) {
        let text = tr!("Not recommended: remapping buttons is still being tested. Keyboard and mouse always keep working.");
        (text.to_owned(), PAD_Y)
    }

    fn apply_fonts(&mut self) {
        let (loaded, warnings) = fonts::install(&self.ctx, &self.settings.fonts.title);
        self.fonts.retain(|f| loaded.contains(f));
        for w in warnings {
            self.notify(w);
        }
        let title = self.settings.fonts.title.clone();
        let missing = !title.is_empty() && !loaded.contains(&title);
        self.fonts_pending = Some(loaded);
        if missing {
            self.notify(tr!("Font {font} is gone; game titles use the default font", font = title));
            self.settings.fonts.title.clear();
            self.save_settings();
        }
    }

    fn set_title_font(&mut self, file: String) {
        self.settings.fonts.title = file;
        self.save_settings();
        self.apply_fonts();
    }

    fn game_font(&self, game: &Game) -> egui::FontFamily {
        match &game.title_font {
            Some(file) if self.fonts.contains(file) => fonts::family(file),
            _ => fonts::title_family(),
        }
    }

    fn game_title_font(&self, game: &Game, size: f32) -> FontId {
        FontId::new(size, self.game_font(game))
    }

    fn import_fonts(&mut self) {
        let Some(paths) = rfd::FileDialog::new().set_title(tr!("Select fonts to import")).add_filter(tr!("Fonts"), fonts::EXTS).pick_files() else {
            return;
        };
        let mut imported = Vec::new();
        for path in paths {
            match fonts::import(&path) {
                Ok(name) => imported.push(name),
                Err(e) => self.notify(tr!("Could not import font: {error}", error = format!("{e:#}"))),
            }
        }
        if imported.is_empty() {
            return;
        }
        self.apply_fonts();
        match imported.as_slice() {
            [one] if self.fonts_pending.as_ref().is_some_and(|f| f.contains(one)) => {
                self.set_title_font(one.clone());
                self.notify(tr!("Imported {font}; game titles now use it", font = fonts::display_name(one)));
            }
            _ => self.notify(tr_n!(imported.len(), "Imported {n} font; choose one under Game title font", "Imported {n} fonts; choose one under Game title font")),
        }
    }

    fn activate_font_row(&mut self) {
        let Mode::FontManager { cursor, armed } = &mut self.mode else { return };
        let Some(file) = self.fonts.get(*cursor).cloned() else {
            return self.import_fonts();
        };
        if !*armed {
            *armed = true;
            return;
        }
        let cursor = *cursor;
        self.delete_font(&file);
        self.mode = Mode::FontManager { cursor: cursor.min(self.fonts.len()), armed: false };
    }

    fn font_usage(&self, file: &str) -> String {
        let games = self.library.games.iter().filter(|g| g.title_font.as_deref() == Some(file)).count();
        let mut uses = Vec::new();
        if self.settings.fonts.title == file {
            uses.push(tr!("Default for game titles").to_owned());
        }
        if games > 0 {
            uses.push(tr_n!(games, "{n} game", "{n} games"));
        }
        if uses.is_empty() { tr!("Not used · {file}", file = file) } else { uses.join(" · ") }
    }

    fn delete_font(&mut self, file: &str) {
        if let Err(e) = fonts::remove(file) {
            return self.notify(tr!("Could not delete font: {error}", error = format!("{e:#}")));
        }
        if self.settings.fonts.title == file {
            self.settings.fonts.title.clear();
            self.save_settings();
        }
        let mut games_changed = false;
        for game in &mut self.library.games {
            if game.title_font.as_deref() == Some(file) {
                game.title_font = None;
                games_changed = true;
            }
        }
        if games_changed {
            self.save();
        }
        self.apply_fonts();
        self.notify(tr!("Deleted {name}", name = fonts::display_name(file)));
    }

    fn theme(&self) -> &Theme {
        &self.themes[self.theme]
    }

    fn button_skin(&self, glyph: &str) -> ButtonSkin {
        let Some(button) = FaceButton::from_glyph(glyph) else { return ButtonSkin::plain(glyph_color(glyph)) };
        let look = self.theme().settings.buttons.get(button);
        ButtonSkin {
            fill: look.color.unwrap_or(glyph_color(glyph)),
            text: look.text_color.unwrap_or(Color32::WHITE),
            icon: self.theme().button_icon(button),
        }
    }

    fn save_theme(&mut self) {
        if let Err(e) = self.themes[self.theme].save() {
            self.notify(tr!("Could not save theme: {error}", error = format!("{e:#}")));
        }
    }

    fn select_theme(&mut self, id: &str) {
        let focused = match self.mode {
            Mode::Settings { cursor, page, in_rows: true } => self.setting_rows(page).get(cursor).copied(),
            _ => None,
        };
        theme::sort(&mut self.themes);
        self.theme = self.themes.iter().position(|t| t.id == id).unwrap_or(0);
        if let Some(row) = focused
            && self.setting_rows(row.page()).contains(&row)
        {
            self.mode = self.settings_at(row);
        }
        if self.settings.theme != self.theme().id {
            self.settings.theme = self.theme().id.clone();
            self.save_settings();
        }
    }

    fn start_store_in_background(&mut self, store: Store) {
        let Some(command) = self.settings.store_command(store) else {
            self.notify(tr!("{store} was not found. Set its command in Settings.", store = store.name()));
            return;
        };
        if store.is_running() {
            return;
        }
        match store.start_in_background(&command) {
            Ok(()) => self.focus_guard_until = Some(Instant::now() + FOCUS_GUARD),
            Err(e) => self.notify(format!("{e:#}")),
        }
    }

    fn poll_discs(&mut self, ctx: &Context) {
        while let Ok(event) = self.discs.try_recv() {
            self.on_disc(ctx, event);
        }
    }

    fn on_disc(&mut self, ctx: &Context, event: DiscEvent) {
        let on_insert = self.settings.discs.on_insert;
        if on_insert == DiscAction::Ignore {
            return;
        }
        let mut manifests = Vec::new();
        for (path, parsed) in event.manifests {
            match parsed {
                Ok(m) => manifests.push(m),
                Err(e) => {
                    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.notify(tr!("{file} on {volume} can't be used: {error}", file = file, volume = volume_name(&event.volume), error = e));
                }
            }
        }
        let Some(first) = manifests.first() else { return };
        let name = first.display_name();
        eprintln!("disc inserted: {name} ({})", event.volume.root.display());
        if self.burning.as_ref().is_some_and(|b| b.result.is_none())
            || self.discs_quiet_until.is_some_and(|until| Instant::now() < until)
        {
            eprintln!("  ignored: just burned");
            return;
        }
        if matches!(self.mode, Mode::Launching(_)) {
            self.notify(tr!("Disc inserted: {name}. Another game is already starting.", name = name));
            return;
        }

        let candidates = match crate::physical::resolve(&manifests, &self.library.disc_ids()) {
            Resolution::NotFound => {
                if self.last_focused.elapsed() < DISC_FOCUS_GRACE {
                    self.focus_guard_until = Some(Instant::now() + FOCUS_GUARD);
                }
                self.mode = Mode::DiscNotFound { name };
                return;
            }
            Resolution::Launch(i) => vec![i],
            Resolution::Choose(candidates) => candidates,
        };
        let in_front = self.last_focused.elapsed() < DISC_FOCUS_GRACE;
        if let [only] = candidates[..]
            && in_front
            && on_insert == DiscAction::Start
        {
            self.mode = Mode::Home;
            self.selected = only;
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            self.launch_selected();
            return;
        }
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.focus_guard_until = Some(Instant::now() + FOCUS_GUARD);
        self.mode = Mode::DiscChooser { name, candidates, cursor: 0 };
    }

    fn create_disc_file(&mut self) {
        let Some(game) = self.selected_game() else { return };
        let manifest = disc_manifest(game);
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!("Save disc file (burn it to the root of a CD/DVD or copy it to a USB stick)"))
            .set_file_name(disc_file_name(game))
            .add_filter(tr!("YASAFF disc file"), &["yasaff"])
            .save_file()
        else {
            return;
        };
        let path = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("yasaff")) { path } else { path.with_extension("yasaff") };
        match std::fs::write(&path, manifest.to_json()) {
            Ok(()) if manifest.ids.is_empty() => self.notify(tr!(
                "Saved {file}. Its launch command has no Steam/GOG/Epic ID, so the disc works only with this library.",
                file = path.display()
            )),
            Ok(()) => self.notify(tr!("Saved {file}. Burn it to the root of a disc.", file = path.display())),
            Err(e) => self.notify(tr!("Could not save {file}: {error}", file = path.display(), error = e)),
        }
    }

    fn open_burner(&mut self) {
        if self.selected_game().is_none() && self.burning.is_none() {
            return;
        }
        let burners = burn::burners();
        let cursor = burners.iter().position(|b| b.media.can_burn()).unwrap_or(0);
        self.mode = Mode::Burn(BurnDialog { burners, looked: Instant::now(), cursor, armed: false });
    }

    fn start_burn(&mut self) {
        let Mode::Burn(dialog) = &mut self.mode else { return };
        let Some(burner) = dialog.burners.get(dialog.cursor).cloned() else { return };
        dialog.armed = false;
        let Some(game) = self.selected_game() else { return };
        let manifest = disc_manifest(game);
        let job = BurnJob {
            device: burner.device.clone(),
            label: game.name.clone(),
            files: vec![(disc_file_name(game), manifest.to_json().into_bytes())],
            media: burner.media.clone(),
        };
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::Builder::new()
            .name("yasaff-burn".into())
            .spawn(move || {
                let result = burn::burn(&job, |p| {
                    let _ = tx.send(BurnMsg::Progress(p));
                    ctx.request_repaint();
                });
                let _ = tx.send(BurnMsg::Done(result));
                ctx.request_repaint();
            })
            .expect("spawning the burn thread");
        eprintln!("burning {} on {} ({})", game.name, burner.device.display(), burner.media);
        self.burning = Some(Burning {
            game: game.name.clone(),
            drive: burner.name,
            rx,
            progress: burn::Progress { fraction: None, status: tr!("Starting").into() },
            result: None,
        });
    }

    fn poll_burn(&mut self) {
        if let Some(b) = &mut self.burning {
            while let Ok(msg) = b.rx.try_recv() {
                match msg {
                    BurnMsg::Progress(p) => b.progress = p,
                    BurnMsg::Done(result) => b.result = Some(result),
                }
            }
            if b.result.is_some() && self.discs_quiet_until.is_none() {
                self.discs_quiet_until = Some(Instant::now() + BURN_QUIET);
                if !matches!(self.mode, Mode::Burn(_)) {
                    let b = self.burning.take().unwrap();
                    match b.result.unwrap() {
                        Ok(()) => self.notify(tr!("Disc for {game} burned", game = b.game)),
                        Err(e) => self.notify(tr!("Burning {game} failed: {error}", game = b.game, error = e)),
                    }
                }
            }
        }
        if let Mode::Burn(dialog) = &mut self.mode
            && self.burning.is_none()
            && dialog.looked.elapsed() >= BURNER_REFRESH
        {
            dialog.burners = burn::burners();
            dialog.looked = Instant::now();
            dialog.cursor = dialog.cursor.min(dialog.burners.len().saturating_sub(1));
        }
    }

    fn close_burner(&mut self) {
        match &self.burning {
            Some(b) if b.result.is_none() => self.notify(tr!("Burning continues in the background")),
            Some(b) if matches!(b.result, Some(Err(_))) => {
                self.burning = None;
                self.discs_quiet_until = None;
                return self.open_burner();
            }
            Some(_) => self.burning = None,
            None => {}
        }
        self.mode = Mode::Home;
    }

    fn handle(&mut self, ctx: &Context, action: Action) {
        match action {
            Action::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
                return;
            }
            Action::BringToFront => return self.come_home(ctx),
            _ => {}
        }

        let (rows, sections) = match self.mode {
            Mode::Settings { page, .. } => {
                let rows = self.setting_rows(page);
                let sections = self.setting_sections(&rows);
                (rows, sections)
            }
            _ => (Vec::new(), Vec::new()),
        };
        match &mut self.mode {
            Mode::Home => match action {
                Action::Left | Action::Right | Action::Up | Action::Down => {
                    if self.is_bubbles() {
                        self.navigate_bubbles(action);
                    } else {
                        self.selected = navigate(self.home_layout(), self.selected, self.tile_count(), action);
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
            },
            Mode::Options { cursor } => match action {
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
            },
            Mode::SwapBubble { from, target } => match action {
                Action::Left | Action::Right | Action::Up | Action::Down => {
                    let from = *target;
                    let to = navigate_free(self.bubble_layout.borrow().as_ref().map_or(&[], |l| &l.centers), from, action);
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
            },
            Mode::Welcome { cursor } => match action {
                Action::Up => *cursor = cursor.saturating_sub(1),
                Action::Down => *cursor = (*cursor + 1).min(WELCOME.len() - 1),
                Action::Confirm => {
                    let start = WELCOME[*cursor].0;
                    self.start_with(start);
                }
                _ => {}
            },
            Mode::AddMenu { cursor } => match action {
                Action::Up => *cursor = cursor.saturating_sub(1),
                Action::Down => *cursor = (*cursor + 1).min(ADD_MENU.len() - 1),
                Action::Confirm => {
                    let source = ADD_MENU[*cursor].0;
                    self.run_add_source(source);
                }
                Action::Back => self.mode = Mode::Home,
                _ => {}
            },
            Mode::Picker(p) => match action {
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
            },
            Mode::Scan(sc) => match action {
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
            },
            Mode::ConfirmRemove { what, yes } => match action {
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
                            self.mode = self.settings_at(SettingRow::AddonImport);
                            if yes {
                                self.uninstall_addon(i);
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
                        Removal::Addon(i) => self.settings_at(SettingRow::AddonRemove(i)),
                        Removal::Profile => self.settings_at(SettingRow::ProfileReset),
                        Removal::Everything => self.settings_at(SettingRow::AppReset),
                    }
                }
                _ => {}
            },
            Mode::Prompt { buffer, .. } => match action {
                Action::Up | Action::Down | Action::Left | Action::Right => self.keyboard.navigate(action),
                Action::Confirm => self.keyboard.press_selected(buffer),
                Action::Erase => self.keyboard.erase(buffer),
                Action::Add => self.keyboard.toggle_shift(),
                Action::Options => self.commit_prompt(),
                Action::Back => self.cancel_prompt(),
                _ => {}
            },
            Mode::Settings { cursor, page, in_rows } => {
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
            Mode::CoverEditor(editor) => {
                let on_buttons = editor.cursor == CoverSlider::ALL.len();
                match action {
                    Action::Up => editor.cursor = editor.cursor.saturating_sub(1),
                    Action::Down => editor.cursor = (editor.cursor + 1).min(CoverSlider::ALL.len()),
                    Action::Left | Action::Right => {
                        let delta = if action == Action::Left { -1 } else { 1 };
                        if on_buttons {
                            editor.button = cycle(&EditorButton::ALL, editor.button, delta);
                        } else {
                            CoverSlider::ALL[editor.cursor].step(&mut editor.draft, delta);
                        }
                    }
                    Action::Add => editor.draft = CoverPlacement::default(),
                    Action::Confirm => match editor.button {
                        EditorButton::Reset if on_buttons => editor.draft = CoverPlacement::default(),
                        EditorButton::Cancel if on_buttons => self.close_cover_editor(false),
                        _ => self.close_cover_editor(true),
                    },
                    Action::Back => self.close_cover_editor(false),
                    _ => {}
                }
            }
            Mode::Launching(_) => {
                if action == Action::Back {
                    self.mode = Mode::Home;
                }
            }
            Mode::DiscChooser { candidates, cursor, .. } => match action {
                Action::Left | Action::Up => *cursor = cursor.saturating_sub(1),
                Action::Right | Action::Down => *cursor = (*cursor + 1).min(candidates.len() - 1),
                Action::Confirm => {
                    self.selected = candidates[*cursor];
                    self.mode = Mode::Home;
                    self.launch_selected();
                }
                Action::Back => self.mode = Mode::Home,
                _ => {}
            },
            Mode::DiscNotFound { .. } => {
                if matches!(action, Action::Confirm | Action::Back) {
                    self.mode = Mode::Home;
                }
            }
            Mode::ProfileImported { name, switch, .. } => match action {
                Action::Left | Action::Right | Action::Up | Action::Down => *switch = !*switch,
                Action::Confirm if *switch => {
                    let name = name.clone();
                    match profiles::switch(&name) {
                        Ok(()) => profiles::restart(),
                        Err(e) => self.notify(tr!("Could not switch profile: {error}", error = format!("{e:#}"))),
                    }
                }
                Action::Confirm | Action::Back => self.mode = self.settings_at(SettingRow::ProfileImport),
                _ => {}
            },
            Mode::Burn(dialog) => {
                if let Some(b) = &self.burning {
                    let done = b.result.is_some();
                    if action == Action::Back || (done && action == Action::Confirm) {
                        self.close_burner();
                    }
                    return;
                }
                let count = dialog.burners.len();
                match action {
                    Action::Up | Action::Left => (dialog.cursor, dialog.armed) = (dialog.cursor.saturating_sub(1), false),
                    Action::Down | Action::Right => {
                        (dialog.cursor, dialog.armed) = ((dialog.cursor + 1).min(count.saturating_sub(1)), false)
                    }
                    Action::Back if dialog.armed => dialog.armed = false,
                    Action::Back => self.close_burner(),
                    Action::Confirm => {
                        let Some(media) = dialog.burners.get(dialog.cursor).map(|b| b.media.clone()) else { return };
                        if media.erases() && !dialog.armed {
                            dialog.armed = true;
                        } else if media.can_burn() {
                            self.start_burn();
                        } else {
                            let why = match media {
                                burn::Media::None => tr!("Insert a blank CD or DVD first").to_owned(),
                                other => tr!("{disc}: insert a blank CD or DVD", disc = other),
                            };
                            self.notify(why);
                        }
                    }
                    _ => {}
                }
            }
            Mode::Store { .. } => self.handle_store(action),
            Mode::StoreTheme { .. } => self.handle_store_theme(action),
            Mode::FontManager { cursor, armed } => {
                let last = self.fonts.len();
                match action {
                    Action::Up => (*cursor, *armed) = (cursor.saturating_sub(1), false),
                    Action::Down => (*cursor, *armed) = ((*cursor + 1).min(last), false),
                    Action::Confirm => self.activate_font_row(),
                    Action::Add => self.import_fonts(),
                    Action::Back if *armed => *armed = false,
                    Action::Back | Action::Options => self.mode = self.settings_at(SettingRow::FontManage),
                    _ => {}
                }
            }
        }
    }

    fn setting_rows(&self, page: Page) -> Vec<SettingRow> {
        use SettingRow as R;
        if page == Page::GameBox {
            let game = self.selected_game();
            let mut rows = vec![R::GameAdjust(AssetKind::Cover)];
            if game.is_some_and(|g| !g.cover_placement.is_default()) {
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
            if game.is_some_and(|g| g.model.is_some()) {
                rows.extend([R::GameModelUp, R::GameModelClear]);
            }
            rows.push(R::GameVideo);
            if game.is_some_and(|g| g.video.is_some()) {
                rows.push(R::GameVideoClear);
            }
            rows.push(R::GameObjectColor);
            rows.extend([R::GameSpineLabel, R::GameSpineColor, R::GameSpineText, R::GameTopColor, R::GameTitleFont]);
            return rows;
        }
        let theme = &self.theme().settings;
        let mut rows = vec![R::Fullscreen, R::Autostart, R::Language];
        rows.extend([R::ProfileActive, R::ProfileRename, R::ProfileNew, R::ProfileExport, R::ProfileImport, R::ProfileReset, R::AppReset]);
        rows.extend([R::ViewKind, R::TileStyle]);
        let bubbles = theme.kind == ThemeKind::Bubbles;
        if bubbles {
            rows.extend([R::BubbleSize, R::BubbleVariance, R::BubbleFill, R::BubbleShuffle, R::BubbleVideo, R::BubbleSound]);
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
        rows.extend([R::AboutApp, R::AboutSource, R::AboutServer]);
        rows
    }

    fn setting_enabled(&self, row: SettingRow) -> bool {
        match row {
            SettingRow::TileBar => self.theme().settings.tiles.rows <= 1,
            SettingRow::ViewKind | SettingRow::TileStyle => !self.is_bubbles(),
            SettingRow::DiscAngle => !self.theme().settings.discs.lying_flat(),
            SettingRow::SpecialPlace(special) => self.special_button(special).is_some(),
            _ => true,
        }
    }

    fn settings_cursor(&self, row: SettingRow) -> usize {
        self.setting_rows(row.page()).iter().position(|r| *r == row).unwrap_or(0)
    }

    fn settings_at(&self, row: SettingRow) -> Mode {
        Mode::Settings { cursor: self.settings_cursor(row), page: row.page(), in_rows: true }
    }

    fn color_slot(&mut self, row: SettingRow) -> Option<&mut Color32> {
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

    fn box_size_slot(row: SettingRow) -> Option<(usize, (f32, f32), f32)> {
        match row {
            SettingRow::BoxWidth => Some((0, theme::BOX_WIDTH_RANGE, 5.0)),
            SettingRow::BoxHeight => Some((1, theme::BOX_HEIGHT_RANGE, 5.0)),
            SettingRow::BoxDepth => Some((2, theme::BOX_DEPTH_RANGE, 1.0)),
            _ => None,
        }
    }

    fn set_box_size(&mut self, row: SettingRow, mm: f32) {
        let Some((i, (min, max), _)) = Self::box_size_slot(row) else { return };
        let boxes = &mut self.themes[self.theme].settings.boxes;
        boxes.custom_size = boxes.size_mm();
        boxes.custom_size[i] = mm.round().clamp(min, max);
        self.save_theme();
    }

    fn button_color_slot(&mut self, row: SettingRow) -> Option<&mut Option<Color32>> {
        let SettingRow::Button(button, field) = row else { return None };
        let look = self.themes[self.theme].settings.buttons.get_mut(button);
        match field {
            ButtonRow::Color => Some(&mut look.color),
            ButtonRow::TextColor => Some(&mut look.text_color),
            _ => None,
        }
    }

    fn game_color_slot(&mut self, row: SettingRow) -> Option<&mut Option<Color32>> {
        let o = &mut self.library.games.get_mut(self.selected)?.box_style;
        match row {
            SettingRow::GameSpineColor => Some(&mut o.spine_color),
            SettingRow::GameSpineText => Some(&mut o.spine_text_color),
            SettingRow::GameTopColor => Some(&mut o.top_color),
            SettingRow::GameObjectColor => Some(&mut o.object_color),
            _ => None,
        }
    }

    fn activate_setting(&mut self, row: SettingRow) {
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
                match profiles::switch(&name) {
                    Ok(()) => profiles::restart(),
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
                let Some(path) = rfd::FileDialog::new()
                    .set_title(tr!("Select cursor image (its top-left corner is the tip)"))
                    .add_filter(tr!("Images"), IMAGE_EXTS)
                    .pick_file()
                else {
                    return;
                };
                if let Err(e) = self.settings.import_cursor(&path) {
                    self.notify(tr!("Could not import cursor: {error}", error = format!("{e:#}")));
                }
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
                let Some(parent) = rfd::FileDialog::new().set_title(tr!("Export theme to folder")).pick_folder() else { return };
                match self.theme().export(&parent) {
                    Ok(dir) => self.notify(tr!("Theme exported to {folder}", folder = dir.display())),
                    Err(e) => self.notify(tr!("Could not export theme: {error}", error = format!("{e:#}"))),
                }
            }
            SettingRow::ThemeImport => {
                let Some(dir) = rfd::FileDialog::new().set_title(tr!("Select an exported theme folder")).pick_folder() else { return };
                match Theme::import(&dir) {
                    Ok(theme) => {
                        let id = theme.id.clone();
                        self.themes.push(theme);
                        self.select_theme(&id);
                        self.notify(tr!("Imported {name}", name = self.theme().settings.name));
                    }
                    Err(e) => self.notify(tr!("Could not import theme: {error}", error = format!("{e:#}"))),
                }
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
            SettingRow::BackgroundImage => {
                let Some(path) = rfd::FileDialog::new()
                    .set_title(tr!("Select background image"))
                    .add_filter(tr!("Images"), IMAGE_EXTS)
                    .pick_file()
                else {
                    return;
                };
                if let Err(e) = self.themes[self.theme].set_background_image(&path) {
                    self.notify(tr!("Could not set image: {error}", error = format!("{e:#}")));
                }
            }
            SettingRow::Bar(slot, BarRow::Image) => {
                let Some(path) = rfd::FileDialog::new()
                    .set_title(tr!("Select bar image"))
                    .add_filter(tr!("Images"), IMAGE_EXTS)
                    .pick_file()
                else {
                    return;
                };
                if let Err(e) = self.themes[self.theme].set_bar_image(slot, &path) {
                    self.notify(tr!("Could not set image: {error}", error = format!("{e:#}")));
                }
            }
            SettingRow::Button(button, ButtonRow::Icon) => {
                let Some(path) = rfd::FileDialog::new()
                    .set_title(tr!("Select button icon"))
                    .add_filter(tr!("Images"), IMAGE_EXTS)
                    .pick_file()
                else {
                    return;
                };
                if let Err(e) = self.themes[self.theme].set_button_icon(button, &path) {
                    self.notify(tr!("Could not set image: {error}", error = format!("{e:#}")));
                }
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
            | SettingRow::Bar(..) => self.adjust_setting(row, 1),
        }
    }

    fn adjust_setting(&mut self, row: SettingRow, delta: i32) {
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

    fn set_view(&mut self, view: ViewKind) {
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

    fn adjust_game_setting(&mut self, row: SettingRow, delta: i32) {
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

    fn close_cover_editor(&mut self, accept: bool) {
        let Mode::CoverEditor(editor) = &self.mode else { return };
        let kind = editor.kind;
        if accept && let Some(game) = self.library.games.get_mut(self.selected) {
            *game.placement_mut(kind) = editor.draft;
            self.save();
        }
        self.mode = self.settings_at(SettingRow::GameAdjust(kind));
    }

    fn delete_theme(&mut self) {
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

    fn activate_selected(&mut self) {
        match self.special(self.selected) {
            None => self.launch_selected(),
            Some(Special::Store) => self.open_store(),
            Some(Special::Add) => self.mode = Mode::AddMenu { cursor: 0 },
            Some(Special::Settings) => self.mode = Mode::Settings { cursor: 0, page: Page::App, in_rows: false },
        }
    }

    fn launch_selected(&mut self) {
        let Some(game) = self.selected_game() else { return };
        if game.app_id.is_some() {
            return self.launch_app();
        }
        if self.game.as_ref().is_some_and(|g| g.game_id == game.id) {
            return self.switch_to_game();
        }
        let (name, cover, font, game_id) = (game.name.clone(), game.cover.clone(), self.game_font(game), game.id.clone());
        match launcher::launch(game, &self.settings) {
            Ok(via) => {
                self.selected = self.library.mark_played(self.selected);
                self.save();
                let watch = GameWatch::start(self.ctx.clone());
                self.set_running_game(RunningGame { game_id, name: name.clone(), watch: watch.clone() });
                self.focus_guard_until = None;
                self.mode = Mode::Launching(Launching { name, font, cover, via, store_app: false, started: Instant::now(), watch });
            }
            Err(e) => self.notify(tr!("Could not start {name}: {error}", name = name, error = format!("{e:#}"))),
        }
    }

    fn update_launching(&mut self, ctx: &Context) {
        let Mode::Launching(l) = &self.mode else { return };
        let elapsed = l.started.elapsed();
        let focused = ctx.input(|i| i.focused);
        if !focused && l.via.is_some() && elapsed < FOCUS_GUARD && !l.watch.seen() {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        } else if elapsed >= LAUNCH_MIN && !focused {
            self.mode = Mode::Home;
        } else if elapsed >= LAUNCH_MIN && l.watch.seen() {
            if l.store_app {
                self.switch_to_running();
            } else {
                self.mode = Mode::Home;
                ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
            }
        } else if elapsed >= LAUNCH_TIMEOUT {
            let msg = tr!("{name} hasn't opened a window yet; it may still be starting.", name = l.name);
            self.mode = Mode::Home;
            self.notify(msg);
        }
    }

    fn guard_focus(&mut self, ctx: &Context) {
        let Some(until) = self.focus_guard_until else { return };
        if Instant::now() >= until || matches!(self.mode, Mode::Launching(_)) {
            self.focus_guard_until = None;
        } else if !ctx.input(|i| i.focused) {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
    }

    fn start_with(&mut self, start: Start) {
        match start {
            Start::Import => {
                initial::mark_done();
                self.mode = Mode::Scan(Box::new(ScanState::start(&self.ctx, scan::Known::of(&self.library), true, true)));
            }
            Start::Blank => {
                initial::mark_done();
                self.mode = Mode::Home;
            }
            Start::Demo => {
                if self.setup.is_none() {
                    self.setup = Some(InitialSetup { rx: initial::start(self.ctx.clone()), total: 0, done: 0 });
                }
                self.mode = Mode::Home;
            }
        }
    }

    fn run_add_source(&mut self, source: AddSource) {
        match source {
            AddSource::Scan => {
                self.mode = Mode::Scan(Box::new(ScanState::start(&self.ctx, scan::Known::of(&self.library), true, false)));
            }
            AddSource::Installed => self.mode = Mode::Picker(Box::new(Picker::new())),
            AddSource::Browse => {
                let picked = rfd::FileDialog::new()
                    .set_title(tr!("Select a game executable, script or shortcut"))
                    .pick_file();
                if let Some(path) = picked {
                    self.import_entry(apps::from_file(&path));
                }
            }
            AddSource::Command => {
                self.mode = Mode::Prompt { kind: PromptKind::AddCommand, buffer: String::new() };
            }
            AddSource::ImportApp => self.import_app_file(),
        }
    }

    fn import_entry(&mut self, entry: AppEntry) {
        if let Some(i) = self.library.find(&entry.target, &entry.args) {
            self.selected = i;
            self.mode = Mode::Home;
            self.notify(tr!("{name} is already in your library", name = self.library.games[i].name));
            return;
        }
        let i = self.add_entry(&entry);
        self.selected = i;
        self.save();
        if self.library.games[i].cover.is_some() {
            self.mode = Mode::Home;
            self.notify(tr!("Added {name}", name = entry.name));
        } else {
            let item = if self.theme().settings.tile_style.is_box() { MenuItem::BoxAppearance } else { MenuItem::SetCover };
            self.mode = Mode::Options { cursor: self.menu_index(item) };
        }
    }

    fn add_entry(&mut self, entry: &AppEntry) -> usize {
        let i = self.library.add(entry);
        self.library.games[i].save_dir = crate::saves::detect(&self.library.games[i]);
        if let Some(icon) = &entry.icon
            && let Err(e) = self.library.set_asset(i, AssetKind::Cover, icon)
        {
            self.notify(tr!("Could not use the shortcut's icon: {error}", error = format!("{e:#}")));
        }
        i
    }

    fn poll_scan(&mut self) {
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

    fn import_scanned(&mut self) {
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

    fn fetch_artwork(&mut self, game_ids: &[String]) {
        for id in game_ids {
            let Some(game) = self.library.games.iter().find(|g| g.id == *id) else { continue };
            let request = metadata::Request { game_id: id.clone(), ids: game.external_ids() };
            self.artwork.fetch(request);
        }
    }

    fn poll_artwork(&mut self) {
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

    fn menu(&self) -> Vec<(MenuItem, String)> {
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
                MenuItem::SaveFolder if store_app => {}
                MenuItem::SwapBubble if !self.is_bubbles() => {}
                MenuItem::SetCover if boxes => {}
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

    fn menu_index(&self, item: MenuItem) -> usize {
        self.menu().iter().position(|(i, _)| *i == item).unwrap_or(0)
    }

    fn run_menu_item(&mut self, item: MenuItem) {
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
            MenuItem::Update => self.update_selected(),
            MenuItem::SetCover => self.pick_asset(AssetKind::Cover),
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
            MenuItem::EditCommand => {
                let buffer = apps::join_command(&game.target, &game.args);
                self.mode = Mode::Prompt { kind: PromptKind::EditCommand, buffer };
            }
            MenuItem::CreateDisc => self.create_disc_file(),
            MenuItem::BurnDisc => self.open_burner(),
            MenuItem::Remove => self.mode = Mode::ConfirmRemove { what: Removal::Game, yes: false },
        }
    }

    fn pick_save_dir(&mut self) {
        let Some(game) = self.selected_game() else { return };
        let start = game.save_dir.clone().filter(|d| d.is_dir()).unwrap_or_else(|| crate::saves::browse_from(game));
        let Some(dir) = rfd::FileDialog::new()
            .set_title(tr!("Choose the folder {name} keeps its saves in", name = game.name))
            .set_directory(start)
            .pick_folder()
        else {
            return;
        };
        let name = game.name.clone();
        self.library.games[self.selected].save_dir = Some(dir.clone());
        self.save();
        self.mode = Mode::Home;
        self.notify(tr!("Saves of {name}: {folder}", name = name, folder = dir.display()));
    }

    fn open_save_dir(&mut self) {
        let Some(dir) = self.selected_game().and_then(|g| g.save_dir.clone()) else { return };
        self.mode = Mode::Home;
        if let Err(e) = open::that_detached(&dir) {
            self.notify(tr!("Could not open {folder}: {error}", folder = dir.display(), error = e));
        }
    }

    fn backup_saves(&mut self) {
        let Some(game) = self.selected_game() else { return };
        let Some(save_dir) = game.save_dir.clone() else { return };
        if self.backup.is_some() {
            return self.notify(tr!("A backup is already being made"));
        }
        let name = game.name.clone();
        let mut dialog = rfd::FileDialog::new().set_title(tr!("Choose where to save the backup of {name}", name = name));
        if let Some(dir) = self.settings.backup_dir.as_ref().filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        let Some(dest) = dialog.pick_folder() else { return };
        self.mode = Mode::Home;
        self.settings.backup_dir = Some(dest.clone());
        self.save_settings();

        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        let game_name = name.clone();
        std::thread::spawn(move || {
            let result = crate::saves::backup(&game_name, &save_dir, &dest).map_err(|e| format!("{e:#}"));
            if tx.send(result).is_ok() {
                ctx.request_repaint();
            }
        });
        self.backup = Some((name.clone(), rx));
        self.notify(tr!("Backing up the saves of {name}…", name = name));
    }

    fn export_profile(&mut self) {
        if self.profile_job.is_some() {
            return self.notify(tr!("A profile export or import is already running"));
        }
        let name = profiles::active();
        let Some(mut dest) = rfd::FileDialog::new()
            .set_title(tr!("Export profile {name}", name = name))
            .add_filter("ZIP", &["zip"])
            .set_file_name(format!("{name}.zip"))
            .save_file()
        else {
            return;
        };
        if dest.extension().is_none_or(|e| !e.eq_ignore_ascii_case("zip")) {
            dest.as_mut_os_string().push(".zip");
        }
        self.start_profile_job(move || ProfileJob::Exported(profiles::export(&dest).map(|()| dest).map_err(|e| format!("{e:#}"))));
        self.notify(tr!("Exporting profile {name}…", name = name));
    }

    fn import_profile(&mut self) {
        if self.profile_job.is_some() {
            return self.notify(tr!("A profile export or import is already running"));
        }
        let Some(path) = rfd::FileDialog::new().set_title(tr!("Select an exported profile")).add_filter("ZIP", &["zip"]).pick_file() else { return };
        self.start_profile_job(move || ProfileJob::Imported(profiles::import(&path).map_err(|e| format!("{e:#}"))));
        self.notify(tr!("Importing profile…"));
    }

    fn start_profile_job(&mut self, job: impl FnOnce() -> ProfileJob + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            if tx.send(job()).is_ok() {
                ctx.request_repaint();
            }
        });
        self.profile_job = Some(rx);
    }

    fn poll_profile_job(&mut self) {
        let Some(rx) = &self.profile_job else { return };
        let job = match rx.try_recv() {
            Ok(job) => job,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => ProfileJob::Exported(Err(tr!("it stopped unexpectedly").to_owned())),
        };
        self.profile_job = None;
        match job {
            ProfileJob::Exported(Ok(zip)) => self.notify(tr!("Profile exported to {file}", file = zip.display())),
            ProfileJob::Exported(Err(e)) => self.notify(tr!("Could not export profile: {error}", error = e)),
            ProfileJob::Imported(Ok(imported)) => {
                self.profiles = profiles::list();
                self.mode = Mode::ProfileImported { name: imported.name, missing: imported.missing, switch: false };
            }
            ProfileJob::Imported(Err(e)) => self.notify(tr!("Could not import profile: {error}", error = e)),
        }
    }

    fn poll_setup(&mut self) {
        let Some(setup) = &mut self.setup else { return };
        let mut installed = Vec::new();
        let finished = loop {
            match setup.rx.try_recv() {
                Ok(initial::Step::Total(n)) => setup.total = n,
                Ok(initial::Step::Installed(manifest)) => {
                    setup.done += 1;
                    installed.push(*manifest);
                }
                Ok(initial::Step::Failed(e)) => {
                    setup.done += 1;
                    eprintln!("initial games: {e}");
                }
                Err(mpsc::TryRecvError::Empty) => break false,
                Err(mpsc::TryRecvError::Disconnected) => break true,
            }
        };
        let selected = self.selected_game().map(|g| g.id.clone());
        for manifest in installed {
            self.add_to_library(manifest, false);
        }
        self.selected = selected.and_then(|id| self.library.position(&id)).unwrap_or(0);
        if finished {
            self.setup = None;
            initial::mark_done();
        }
    }

    fn draw_setup(&self, painter: &Painter, screen: Rect, s: f32) {
        let Some(setup) = &self.setup else { return };
        if matches!(self.mode, Mode::Scan(_)) {
            return;
        }
        let panel = Rect::from_center_size(pos2(screen.center().x, screen.bottom() - 200.0 * s), vec2(420.0 * s, 86.0 * s));
        painter.rect_filled(panel, (6.0 * s) as u8, Color32::from_black_alpha(220));
        let left = panel.left() + 20.0 * s;
        painter.text(pos2(left, panel.top() + 16.0 * s), Align2::LEFT_TOP, tr!("Setting up initial stuff…"), FontId::proportional(20.0 * s), Color32::WHITE);
        let count = format!("{} / {}", setup.done, setup.total.max(setup.done));
        painter.text(pos2(panel.right() - 20.0 * s, panel.top() + 18.0 * s), Align2::RIGHT_TOP, count, FontId::proportional(17.0 * s), Color32::from_gray(150));
        let bar = Rect::from_min_size(pos2(left, panel.bottom() - 30.0 * s), vec2(panel.width() - 40.0 * s, 12.0 * s));
        let radius = (6.0 * s) as u8;
        painter.rect_filled(bar, radius, Color32::from_white_alpha(25));
        let fraction = if setup.total == 0 { 0.0 } else { setup.done as f32 / setup.total as f32 };
        if fraction > 0.0 {
            painter.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * fraction.min(1.0), bar.height())), radius, ACCENT);
        }
    }

    fn reset_profiles(&mut self, everything: bool) {
        let result = if everything {
            if self.autostart {
                let _ = autostart::set(false);
            }
            profiles::reset_all()
        } else {
            profiles::reset_current()
        };
        match result {
            Ok(()) => profiles::restart(),
            Err(e) => self.notify(tr!("Could not reset: {error}", error = format!("{e:#}"))),
        }
    }

    fn poll_backup(&mut self) {
        let Some((name, rx)) = &self.backup else { return };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(tr!("the backup stopped unexpectedly").to_owned()),
        };
        let name = name.clone();
        self.backup = None;
        match result {
            Ok(zip) => self.notify(tr!("Backed up the saves of {name} to {file}", name = name, file = zip.display())),
            Err(e) => self.notify(tr!("Could not back up the saves of {name}: {error}", name = name, error = e)),
        }
    }

    fn find_missing_save_dirs(&mut self) {
        let games: Vec<Game> = self.library.games.iter().filter(|g| g.save_dir.is_none()).cloned().collect();
        if games.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let finder = crate::saves::Finder::new();
            let found: Vec<_> = games.iter().filter_map(|g| Some((g.id.clone(), finder.find(g)?))).collect();
            if tx.send(found).is_ok() {
                ctx.request_repaint();
            }
        });
        self.save_dirs_found = Some(rx);
    }

    fn poll_save_dirs(&mut self) {
        let Some(found) = self.save_dirs_found.as_ref().and_then(|rx| rx.try_recv().ok()) else { return };
        self.save_dirs_found = None;
        let mut changed = false;
        for (id, dir) in found {
            if let Some(i) = self.library.position(&id)
                && self.library.games[i].save_dir.is_none()
            {
                self.library.games[i].save_dir = Some(dir);
                changed = true;
            }
        }
        if changed {
            self.save();
        }
    }

    fn pick_asset(&mut self, kind: AssetKind) {
        if let AssetKind::Model = kind {
            return self.pick_model();
        }
        if let AssetKind::Video = kind {
            let Some(path) = rfd::FileDialog::new().set_title(tr!("Select preview video")).add_filter(tr!("Videos"), video::EXTS).pick_file() else {
                return;
            };
            return match self.library.set_asset(self.selected, kind, &path) {
                Ok(()) => self.save(),
                Err(e) => self.notify(tr!("Could not use the video: {error}", error = format!("{e:#}"))),
            };
        }
        let title = match kind {
            AssetKind::Cover => tr!("Select cover image (square works best)"),
            AssetKind::Background => tr!("Select background image (16:9 works best)"),
            AssetKind::Spine => tr!("Select spine image (a tall or wide scan of the box spine)"),
            AssetKind::Top => tr!("Select box top image"),
            AssetKind::Back => tr!("Select back cover image"),
            AssetKind::Side => tr!("Select right side image (the edge opposite the spine)"),
            AssetKind::Bottom => tr!("Select box bottom image"),
            AssetKind::Booklet => tr!("Select instruction booklet image"),
            AssetKind::Model | AssetKind::Video => unreachable!(),
        };
        let Some(path) = rfd::FileDialog::new().set_title(title).add_filter(tr!("Images"), IMAGE_EXTS).pick_file() else {
            return;
        };
        match self.library.set_asset(self.selected, kind, &path) {
            Ok(()) => self.save(),
            Err(e) => self.notify(tr!("Could not set image: {error}", error = format!("{e:#}"))),
        }
    }

    fn pick_model(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!("Select a 3D model (STL, OBJ, PLY, glTF / GLB or 3MF)"))
            .add_filter(tr!("3D models"), object3d::EXTS)
            .pick_file()
        else {
            return;
        };
        match self.library.set_model(self.selected, &path) {
            Ok(()) => self.save(),
            Err(e) => self.notify(tr!("Could not use the model: {error}", error = format!("{e:#}"))),
        }
    }

    fn commit_prompt(&mut self) {
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
            PromptKind::AppStoreUrl | PromptKind::AppStoreKey => {
                let row = if kind == PromptKind::AppStoreUrl { SettingRow::AppStoreUrl } else { SettingRow::AppStoreKey };
                let store = self.settings.app_store_mut();
                let slot = match kind {
                    PromptKind::AppStoreUrl => &mut store.url,
                    _ => &mut store.api_key,
                };
                *slot = text.to_owned();
                store.name.clear();
                self.save_settings();
                self.store_settings_changed();
                self.mode = self.settings_at(row);
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
                let result = if text == profiles::active() { Ok(()) } else { profiles::rename(text) };
                self.mode = self.settings_at(SettingRow::ProfileRename);
                match result {
                    Ok(()) if text != profiles::active() => profiles::restart(),
                    Ok(()) => {}
                    Err(e) => self.notify(tr!("Could not rename profile: {error}", error = format!("{e:#}"))),
                }
            }
            PromptKind::ProfileNew => {
                self.mode = self.settings_at(SettingRow::ProfileNew);
                match profiles::create(text).and_then(|()| profiles::switch(text)) {
                    Ok(()) => profiles::restart(),
                    Err(e) => self.notify(tr!("Could not create profile: {error}", error = format!("{e:#}"))),
                }
            }
            PromptKind::Rename => {}
        }
    }

    fn cancel_prompt(&mut self) {
        self.mode = match &self.mode {
            Mode::Prompt { kind: PromptKind::AddCommand, .. } => Mode::AddMenu { cursor: add_menu_index(AddSource::Command) },
            Mode::Prompt { kind: PromptKind::StoreCommand(store), .. } => self.settings_at(SettingRow::StoreCommand(*store)),
            Mode::Prompt { kind: PromptKind::ThemeName, .. } => self.settings_at(SettingRow::ThemeRename),
            Mode::Prompt { kind: PromptKind::ProfileName, .. } => self.settings_at(SettingRow::ProfileRename),
            Mode::Prompt { kind: PromptKind::ProfileNew, .. } => self.settings_at(SettingRow::ProfileNew),
            Mode::Prompt { kind: PromptKind::AppStoreUrl, .. } => self.settings_at(SettingRow::AppStoreUrl),
            Mode::Prompt { kind: PromptKind::AppStoreKey, .. } => self.settings_at(SettingRow::AppStoreKey),
            Mode::Prompt { kind: PromptKind::Color(row) | PromptKind::BoxSize(row), .. } => self.settings_at(*row),
            _ => Mode::Home,
        };
    }

    fn bubble_places(&self) -> Vec<usize> {
        let games = &self.library.games;
        let (n, count) = (games.len(), self.tile_count());
        let total = self.bubble_layout.borrow().as_ref().map_or(count, |l| l.centers.len()).max(count);
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

    fn slot_key(&self, slot: usize) -> Option<String> {
        let (n, count) = (self.library.games.len(), self.tile_count());
        match slot {
            _ if slot < n => Some(self.library.games[slot].id.clone()),
            _ if slot >= count => Some(format!("empty-{}", slot - count)),
            _ => None,
        }
    }

    fn move_bubble(&mut self, from: usize, target: usize) {
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

    fn remove_selected(&mut self) {
        if self.selected < self.library.games.len() {
            self.library.remove(self.selected);
            self.selected = self.selected.min(self.library.games.len());
            self.save();
        }
    }

    fn draw_background(&mut self, ui: &Ui, screen: Rect) {
        let wanted = if self.theme().settings.background.game_art {
            self.selected_game().and_then(|g| g.background.clone())
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

    fn bar_rect(&self, slot: BarSlot, screen: Rect, s: f32) -> Rect {
        let bar = self.theme().settings.bar(slot);
        let h = if bar.enabled { bar.height(slot) } else { slot.default_height() };
        let h = self.bar_minimum(slot).map_or(h, |(min, _)| h.max(min)) * s;
        match slot {
            BarSlot::Top => Rect::from_min_max(screen.min, pos2(screen.right(), screen.top() + h)),
            BarSlot::Bottom => Rect::from_min_max(pos2(screen.left(), screen.bottom() - h), screen.max),
        }
    }

    fn bar_minimum(&self, slot: BarSlot) -> Option<(f32, String)> {
        self.addons.min_height(&self.settings, slot).map(|(h, name)| (h, name.to_owned()))
    }

    fn draw_bars(&self, ui: &Ui, screen: Rect, s: f32) {
        for slot in BarSlot::ALL {
            paint_bar(ui, self.theme(), slot, self.bar_rect(slot, screen, s));
        }
    }

    fn draw_widgets(&self, painter: &Painter, screen: Rect, s: f32) {
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

    fn draw_top_bar(&self, painter: &Painter, screen: Rect, s: f32) {
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

    fn home_layout(&self) -> Layout {
        let theme = &self.theme().settings;
        match theme.view {
            ViewKind::Row => Layout::Row { rows: theme.tiles.rows.max(1) as usize },
            ViewKind::Grid => Layout::Grid { columns: theme.grid.columns.max(1) as usize, rows: theme.grid.rows.max(1) as usize },
        }
    }

    fn draw_row(&mut self, ui: &Ui, screen: Rect, s: f32) {
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

    fn draw_grid(&mut self, ui: &Ui, screen: Rect, s: f32) {
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

    fn is_bubbles(&self) -> bool {
        self.theme().settings.kind == ThemeKind::Bubbles
    }

    /// The tile sitting in a bubble slot; `None` for an empty bubble.
    fn bubble_occupant(&self, slot: usize) -> Option<usize> {
        let specials = self.library.games.len()..self.tile_count();
        self.bubble_places().iter().position(|&p| p == slot).or_else(|| specials.contains(&slot).then_some(slot))
    }

    /// The empty bubble the cursor rests on in Home, if it isn't on a tile.
    fn empty_bubble(&self) -> Option<usize> {
        let slot = self.bubble_empty.filter(|_| self.is_bubbles() && matches!(self.mode, Mode::Home))?;
        let total = self.bubble_layout.borrow().as_ref().map_or(0, |l| l.centers.len());
        (slot < total && self.bubble_occupant(slot).is_none()).then_some(slot)
    }

    fn navigate_bubbles(&mut self, action: Action) {
        let from = self.empty_bubble().unwrap_or_else(|| self.bubble_places().get(self.selected).copied().unwrap_or(self.selected));
        let to = match self.bubble_layout.borrow().as_ref() {
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
    fn bubble_sound(&mut self, slot: usize) {
        if !self.theme().settings.bubbles.sound {
            return;
        }
        let pitch = {
            let layout = self.bubble_layout.borrow();
            let Some(layout) = layout.as_ref() else { return };
            let Some(&r) = layout.radii.get(slot) else { return };
            let mean = layout.radii.iter().sum::<f32>() / layout.radii.len() as f32;
            (620.0 * (mean / r.max(1.0)).powf(0.7)).clamp(300.0, 1400.0)
        };
        self.sounds.bubble(pitch);
    }

    fn bubble_key(&self, i: usize) -> String {
        match self.library.games.get(i) {
            Some(game) => game.id.clone(),
            None => {
                let special = self.special(i);
                format!("special-{}", Special::ALL.iter().position(|s| Some(*s) == special).unwrap_or(0))
            }
        }
    }

    fn update_video(&mut self, ctx: &Context) {
        let theme = &self.theme().settings;
        let wanted = (theme.kind == ThemeKind::Bubbles
            && theme.bubbles.play_video
            && matches!(self.mode, Mode::Home)
            && self.empty_bubble().is_none()
            && ctx.input(|i| i.focused))
        .then(|| self.selected_game().and_then(|g| g.video.clone()))
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

    fn draw_bubbles(&mut self, ui: &Ui, screen: Rect, s: f32) {
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
        let (centers, radii) = {
            let mut cache = self.bubble_layout.borrow_mut();
            if cache.as_ref().is_none_or(|c| c.diameters != diameters || c.area != area) {
                let (centers, radii) = pack_bubbles(&diameters, area);
                *cache = Some(BubbleLayout { diameters, area, centers, radii });
            }
            let layout = cache.as_ref().unwrap();
            (layout.centers.clone(), layout.radii.clone())
        };
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
            let cover = game.and_then(|g| g.cover.as_deref()).and_then(|path| {
                match egui::Image::new(file_uri(path)).load_for_size(&ctx, Vec2::splat(r * 2.0)) {
                    Ok(egui::load::TexturePoll::Ready { texture }) => Some((texture.id, texture.size)),
                    _ => None,
                }
            });
            let look = BubbleLook {
                empty: tile.is_none(),
                texture: video.filter(|_| tile == Some(selected) && game.is_some_and(|g| g.video.is_some())).or(cover),
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

    fn paint_tile(&self, ui: &Ui, i: usize, rect: Rect, radius: u8, s: f32, focus: f32) {
        self.paint_running_glow(ui, i, rect, s);
        self.paint_tile_art(ui, i, rect, radius, s, focus);
        self.paint_artwork_badge(ui, i, rect, s);
        self.paint_app_badges(ui, i, rect, s);
    }

    fn paint_tile_art(&self, ui: &Ui, i: usize, rect: Rect, radius: u8, s: f32, focus: f32) {
        match self.theme().settings.tile_style {
            TileStyle::Flat | TileStyle::Cover => self.paint_flat(ui, i, rect, radius, s, focus),
            TileStyle::FlatBox => self.paint_box(ui, i, rect, s, focus),
            TileStyle::Model => self.paint_model(ui, i, rect, s, focus),
            TileStyle::Object => self.paint_object(ui, i, rect, s, focus),
            TileStyle::Disc => self.paint_disc(ui, i, rect, s, focus),
            TileStyle::Case => self.paint_case(ui, i, rect, s, focus),
        }
    }

    fn paint_artwork_badge(&self, ui: &Ui, i: usize, rect: Rect, s: f32) {
        let Some(status) = self.library.games.get(i).and_then(|g| self.artwork.status(&g.id)) else { return };
        let r = (rect.width() * 0.09).clamp(11.0 * s, 20.0 * s);
        let center = rect.right_top() + vec2(-r - 6.0 * s, r + 6.0 * s);
        let painter = ui.painter();
        let glyph = |fill: Color32, text: &str| {
            painter.circle_filled(center, r, fill);
            painter.text(center, Align2::CENTER_CENTER, text, FontId::proportional(r * 1.2), Color32::WHITE);
        };
        match status {
            metadata::Status::Queued | metadata::Status::Fetching => {
                painter.circle_filled(center, r, Color32::from_black_alpha(190));
                let color = if matches!(status, metadata::Status::Fetching) { Color32::WHITE } else { Color32::from_gray(140) };
                let size = r * 1.3;
                egui::Spinner::new().size(size).color(color).paint_at(ui, Rect::from_center_size(center, vec2(size, size)));
            }
            metadata::Status::Done(at, _) if at.elapsed() < metadata::DONE_BADGE => glyph(PAD_A, "✔"),
            metadata::Status::Done(..) => {}
            metadata::Status::Failed(_) => glyph(PAD_B, "!"),
        }
    }

    fn paint_flat(&self, ui: &Ui, i: usize, rect: Rect, radius: u8, s: f32, focus: f32) {
        let painter = ui.painter();
        painter.rect_filled(rect.translate(vec2(0.0, 8.0 * s)), radius, Color32::from_black_alpha(90));
        match self.library.games.get(i) {
            Some(game) => draw_game_tile(ui, game, rect, radius),
            None => {
                let glyph = self.special(i).map_or("", Special::glyph);
                painter.rect_filled(rect, radius, Color32::from_white_alpha(28));
                painter.text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(rect.height() * 0.4), Color32::WHITE);
            }
        }
        if i == self.selected {
            let stroke = Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4)));
            painter.rect_stroke(rect, radius, stroke, StrokeKind::Outside);
        }
    }

    fn paint_box(&self, ui: &Ui, i: usize, rect: Rect, s: f32, focus: f32) {
        let game = self.library.games.get(i);
        let defaults = &self.theme().settings.boxes;
        let look = game.map_or_else(|| defaults.clone(), |g| defaults.with(&g.box_style));
        let b = BoxShape::fit(rect, look.size_mm());
        let painter = ui.painter();
        let outline = b.outline();

        let shadow: Vec<Pos2> = outline.iter().map(|p| *p + vec2(0.0, 8.0 * s)).collect();
        painter.add(Shape::convex_polygon(shadow, Color32::from_black_alpha(90), Stroke::NONE));

        const TOP_SHADE: f32 = 0.9;
        const SPINE_SHADE: f32 = 0.75;
        let face_image = |kind: AssetKind, quad: [Pos2; 4], color: Color32, shade: f32| {
            painter.add(Shape::convex_polygon(quad.to_vec(), box3d::shade(color, shade), Stroke::NONE));
            game.and_then(|g| Some((g.asset(kind)?, g.placement(kind))))
                .is_some_and(|(path, placement)| paint_image_quad(ui, path, quad, box3d::shade(Color32::WHITE, shade), placement))
        };
        face_image(AssetKind::Top, b.top(), look.top_color, TOP_SHADE);
        if !face_image(AssetKind::Spine, b.spine(), look.spine_color, SPINE_SHADE) && look.spine_label == SpineLabel::Name {
            let (name, font) = match game {
                Some(g) => (g.name.as_str(), self.game_font(g)),
                None => (self.special(i).map_or("", Special::name), egui::FontFamily::Proportional),
            };
            paint_spine_text(ui, name, font, look.spine_text_color, &b);
        }
        match game {
            Some(game) => draw_game_tile(ui, game, b.front, 0),
            None => {
                let glyph = self.special(i).map_or("", Special::glyph);
                painter.rect_filled(b.front, 0, Color32::from_rgb(0x2a, 0x2d, 0x34));
                painter.text(b.front.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(b.front.width() * 0.45), Color32::WHITE);
            }
        }

        let edge = Stroke::new(1.0, Color32::from_black_alpha(110));
        painter.line_segment([b.front.left_top() + b.depth, b.front.left_top()], Stroke::new(1.0, Color32::from_white_alpha(40)));
        painter.line_segment([b.front.left_top(), b.front.left_bottom()], edge);
        painter.line_segment([b.front.left_top(), b.front.right_top()], edge);
        if i == self.selected {
            let stroke = Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4)));
            painter.add(Shape::closed_line(outline.to_vec(), stroke));
        }
    }

    fn turn(&self, i: usize, rest: f32) -> f32 {
        self.turn_every(i, rest, SPIN_TIME.as_secs_f32())
    }

    fn turn_every(&self, i: usize, rest: f32, period: f32) -> f32 {
        if i != self.selected || matches!(self.mode, Mode::CoverEditor(_)) {
            return rest;
        }
        let (tile, since) = self.spin.get();
        if tile != i {
            self.spin.set((i, Instant::now()));
        }
        let turns = since.elapsed().as_secs_f32() / period;
        rest - turns.fract() * std::f32::consts::TAU
    }

    fn paint_object(&self, ui: &Ui, i: usize, rect: Rect, s: f32, focus: f32) {
        let game = self.library.games.get(i);
        let painter = ui.painter();
        let (floor, radius) = object_floor(rect);
        painter.add(Shape::ellipse_filled(floor, radius, Color32::from_black_alpha(90)));
        if i == self.selected {
            let stroke = Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4)));
            painter.add(Shape::ellipse_stroke(floor, radius, stroke));
        }

        let name = match game {
            Some(g) => g.name.as_str(),
            None => self.special(i).map_or("", Special::glyph),
        };
        let title = object3d::Source::Title { text: name.to_owned(), font: self.title_font_file(game) };
        let mut objects = self.objects.borrow_mut();
        let model = match game.and_then(|g| Some(object3d::Source::File(g.model.clone()?, g.model_up))) {
            Some(file) => objects.model(&file).or_else(|()| objects.model(&title)),
            None => objects.model(&title),
        };
        let model = match model {
            Ok(Some(model)) => model,
            Ok(None) => return,
            Err(()) => {
                painter.text(rect.center(), Align2::CENTER_CENTER, name, FontId::proportional(rect.height() * 0.15), Color32::WHITE);
                return;
            }
        };

        let area = Rect::from_min_max(rect.min, pos2(rect.right(), floor.y + radius.y));
        let ppp = ui.ctx().pixels_per_point();
        let pixels = [area.width(), area.height()].map(|v| ((v * ppp / 8.0).ceil() as usize).max(1) * 8);
        let color = game.and_then(|g| g.box_style.object_color).unwrap_or(self.theme().settings.objects.color);
        let tile = game.map_or_else(|| name.to_owned(), |g| g.id.clone());
        let texture = objects.texture(&tile, &model, pixels, self.turn(i, object3d::REST_YAW), color);
        let shown = Rect::from_center_size(area.center(), vec2(pixels[0] as f32, pixels[1] as f32) / ppp);
        painter.image(texture, shown, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }

    fn title_font_file(&self, game: Option<&Game>) -> Option<PathBuf> {
        let loaded = |f: &&String| self.fonts.contains(f);
        let file = game.and_then(|g| g.title_font.as_ref()).filter(loaded).or(Some(&self.settings.fonts.title).filter(loaded))?;
        Some(fonts::dir().join(file))
    }

    fn paint_model(&self, ui: &Ui, i: usize, rect: Rect, s: f32, focus: f32) {
        let (yaw, pitch) = match &self.mode {
            Mode::CoverEditor(editor) if i == self.selected => match editor.kind {
                AssetKind::Back => (box3d::REST_YAW + std::f32::consts::PI, box3d::PITCH),
                AssetKind::Side => (-box3d::REST_YAW, box3d::PITCH),
                AssetKind::Bottom => (box3d::REST_YAW, -box3d::PITCH * 2.0),
                _ => (box3d::REST_YAW, box3d::PITCH),
            },
            _ => (self.turn(i, box3d::REST_YAW), box3d::PITCH),
        };
        let stroke = (i == self.selected).then(|| Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4))));
        self.with_box_art(i, |art, look| box3d::paint(ui, rect, look.size_mm(), (yaw, pitch), art, vec2(0.0, 8.0 * s), stroke));
    }

    fn with_box_art<R>(&self, i: usize, f: impl FnOnce(&box3d::Art, &crate::theme::BoxSettings) -> R) -> R {
        let game = self.library.games.get(i);
        let defaults = &self.theme().settings.boxes;
        let look = game.map_or_else(|| defaults.clone(), |g| defaults.with(&g.box_style));

        use box3d::{Fill, Label};
        let placeholder = game.map_or(Color32::from_rgb(0x2a, 0x2d, 0x34), |g| placeholder_color(&g.name));
        let front_text = match game {
            Some(g) => initials(&g.name),
            None => self.special(i).map_or("", Special::glyph).to_owned(),
        };
        let name = match game {
            Some(g) => g.name.as_str(),
            None => self.special(i).map_or("", Special::name),
        };
        let font = game.map_or(egui::FontFamily::Proportional, |g| self.game_font(g));
        let cover = game.and_then(|g| g.cover.as_deref());
        let spine = game.and_then(|g| g.spine.as_deref());
        let image = |kind: AssetKind, fallback: Color32| match game.and_then(|g| Some((g.asset(kind)?, g.placement(kind)))) {
            Some((path, placement)) if matches!(kind, AssetKind::Cover | AssetKind::Back) => Fill::Cover(path, fallback, placement),
            Some((path, placement)) => Fill::Image(path, fallback, placement),
            None => Fill::Color(fallback),
        };
        let art = box3d::Art {
            front: image(AssetKind::Cover, placeholder),
            front_label: cover.is_none().then(|| Label { text: &front_text, font: font.clone(), color: Color32::WHITE, size: 0.3, down: false }),
            back: image(AssetKind::Back, look.spine_color),
            spine: image(AssetKind::Spine, look.spine_color),
            spine_label: (spine.is_none() && look.spine_label == SpineLabel::Name)
                .then(|| Label { text: name, font: font.clone(), color: look.spine_text_color, size: 0.5, down: true }),
            top: image(AssetKind::Top, look.top_color),
            side: image(AssetKind::Side, look.top_color),
            bottom: image(AssetKind::Bottom, look.top_color),
            plain: look.top_color,
        };
        f(&art, &look)
    }

    fn paint_case(&self, ui: &Ui, i: usize, rect: Rect, s: f32, focus: f32) {
        let pose = match &self.mode {
            Mode::CoverEditor(editor) if i == self.selected && editor.kind == AssetKind::Booklet => case3d::Pose::opening(1.0, 0.0),
            Mode::CoverEditor(_) => case3d::Pose::CLOSED,
            _ => {
                let ctx = ui.ctx();
                let progress = ctx.animate_bool_with_time(Id::new(("case-open", i)), i == self.selected, case3d::OPEN_TIME);
                let sway = ctx.input(|input| input.time) as f32 / case3d::SWAY_PERIOD * std::f32::consts::TAU;
                case3d::Pose::opening(progress, sway)
            }
        };
        let stroke = (i == self.selected).then(|| Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4))));
        let game = self.library.games.get(i);
        let placeholder = game.map_or(Color32::from_rgb(0x2a, 0x2d, 0x34), |g| placeholder_color(&g.name));
        let booklet = match game.and_then(|g| Some((g.booklet.as_deref().or(g.cover.as_deref())?, g))) {
            Some((path, g)) => box3d::Fill::Cover(path, placeholder, g.placement(if g.booklet.is_some() { AssetKind::Booklet } else { AssetKind::Cover })),
            None => box3d::Fill::Color(placeholder),
        };
        self.with_box_art(i, |case, look| {
            self.with_disc_art(i, |disc| {
                let art = case3d::Art { case, disc, booklet, plastic: look.top_color };
                case3d::paint(ui, rect, look.size_mm(), pose, &art, Some(vec2(0.0, 8.0 * s)), stroke);
            })
        });
    }

    fn disc_pose(&self, i: usize) -> disc3d::Pose {
        let discs = &self.theme().settings.discs;
        let turn = if discs.spin { self.turn_every(i, 0.0, 60.0 / discs.spin_speed.max(0.1)) } else { 0.0 };
        disc3d::Pose { tilt: discs.shown_angle().to_radians(), spin: discs.spin_direction, turn }
    }

    fn paint_disc(&self, ui: &Ui, i: usize, rect: Rect, s: f32, focus: f32) {
        let stroke = (i == self.selected).then(|| Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus.max(0.4))));
        self.with_disc_art(i, |art| disc3d::paint(ui, rect, self.disc_pose(i), art, vec2(0.0, 8.0 * s), stroke));
    }

    fn with_disc_art<R>(&self, i: usize, f: impl FnOnce(&disc3d::Art) -> R) -> R {
        let game = self.library.games.get(i);
        let text = match game {
            Some(g) => initials(&g.name),
            None => self.special(i).map_or("", Special::glyph).to_owned(),
        };
        let placeholder = game.map_or(Color32::from_rgb(0x2a, 0x2d, 0x34), |g| placeholder_color(&g.name));
        let cover = game.and_then(|g| g.cover.as_deref());
        let font = game.map_or(egui::FontFamily::Proportional, |g| self.game_font(g));
        let art = disc3d::Art {
            kind: self.theme().settings.discs.kind,
            print: match cover {
                Some(path) => box3d::Fill::Cover(path, placeholder, game.map(|g| g.cover_placement).unwrap_or_default()),
                None => box3d::Fill::Color(placeholder),
            },
            label: cover.is_none().then(|| disc3d::Label { text: &text, font, color: Color32::WHITE, size: 0.2 }),
        };
        f(&art)
    }

    fn draw_details(&self, ui: &Ui, pos: egui::Pos2, s: f32, start_button: bool) -> bool {
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

    fn click_tile(&mut self, i: usize) {
        if i == self.selected {
            self.activate_selected();
        } else {
            self.selected = i;
        }
    }

    fn draw_hints(&self, ui: &Ui, screen: Rect, s: f32) -> Option<Action> {
        let hints: &[Hint] = match self.mode {
            Mode::Home if self.empty_bubble().is_some() => &[],
            Mode::Home if self.app_running_at(self.selected) => &[
                Hint(Action::Confirm, tr!("Switch to app")),
                Hint(Action::Options, tr!("Options")),
            ],
            Mode::Home if self.game_index() == Some(self.selected) => &[
                Hint(Action::Confirm, tr!("Switch to game")),
                Hint(Action::Options, tr!("Options")),
            ],
            Mode::Home if self.selected_game().is_some() => &[
                Hint(Action::Confirm, tr!("Start")),
                Hint(Action::Add, tr!("Add game")),
                Hint(Action::Options, tr!("Options")),
            ],
            Mode::Home if self.special(self.selected) == Some(Special::Add) => &[Hint(Action::Confirm, tr!("Add game"))],
            Mode::Home if self.special(self.selected) == Some(Special::Store) => &[Hint(Action::Confirm, tr!("Open Store"))],
            Mode::Store { .. } if self.store_on_themes() => &[
                Hint(Action::Confirm, tr!("Open")),
                Hint(Action::Options, tr!("Apps")),
                Hint(Action::Add, tr!("Refresh")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Store { .. } => &[
                Hint(Action::Confirm, tr!("Get / Update / Start")),
                Hint(Action::Options, tr!("Themes")),
                Hint(Action::Add, tr!("Refresh")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::StoreTheme { .. } => &[
                Hint(Action::Confirm, tr!("Install / Use")),
                Hint(Action::Left, tr!("Pictures")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Home => &[Hint(Action::Confirm, tr!("Open"))],
            Mode::Settings { in_rows: false, .. } => &[
                Hint(Action::Confirm, tr!("Open")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Settings { .. } => &[
                Hint(Action::Confirm, tr!("Change")),
                Hint(Action::Left, tr!("Adjust")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Launching(_) => &[Hint(Action::Back, tr!("Hide"))],
            Mode::SwapBubble { target, .. } if !self.bubble_places().contains(&target) && target >= self.tile_count() => {
                &[Hint(Action::Confirm, tr!("Move here")), Hint(Action::Back, tr!("Cancel"))]
            }
            Mode::SwapBubble { .. } => &[Hint(Action::Confirm, tr!("Swap here")), Hint(Action::Back, tr!("Cancel"))],
            Mode::Options { .. } | Mode::AddMenu { .. } | Mode::ConfirmRemove { .. } => {
                &[Hint(Action::Confirm, tr!("Select")), Hint(Action::Back, tr!("Back"))]
            }
            Mode::Picker(_) => &[
                Hint(Action::Confirm, tr!("Add")),
                Hint(Action::Left, tr!("Category")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Scan(_) => &[
                Hint(Action::Confirm, tr!("Select")),
                Hint(Action::Add, tr!("All / none")),
                Hint(Action::Options, tr!("Scan again")),
                Hint(Action::Back, tr!("Back")),
            ],
            Mode::Prompt { .. } if self.input.has_gamepad() => &[
                Hint(Action::Confirm, tr!("Type")),
                Hint(Action::Erase, tr!("Delete")),
                Hint(Action::Add, tr!("Shift")),
                Hint(Action::Options, tr!("Save")),
                Hint(Action::Back, tr!("Cancel")),
            ],
            Mode::Prompt { .. } => &[Hint(Action::Confirm, tr!("Save")), Hint(Action::Back, tr!("Cancel"))],
            Mode::CoverEditor(_) => &[
                Hint(Action::Confirm, tr!("Accept")),
                Hint(Action::Left, tr!("Adjust")),
                Hint(Action::Add, tr!("Reset")),
                Hint(Action::Back, tr!("Cancel")),
            ],
            Mode::DiscChooser { .. } => &[Hint(Action::Confirm, tr!("Start")), Hint(Action::Back, tr!("Cancel"))],
            Mode::DiscNotFound { .. } => &[Hint(Action::Confirm, tr!("OK"))],
            Mode::Welcome { .. } => &[Hint(Action::Confirm, tr!("Select"))],
            Mode::ProfileImported { .. } => &[Hint(Action::Confirm, tr!("Select")), Hint(Action::Back, tr!("Back"))],
            Mode::Burn(_) => match &self.burning {
                Some(Burning { result: Some(Err(_)), .. }) => &[Hint(Action::Confirm, tr!("Try again"))],
                Some(b) if b.result.is_some() => &[Hint(Action::Confirm, tr!("OK"))],
                Some(_) => &[Hint(Action::Back, tr!("Hide"))],
                None => match &self.mode {
                    Mode::Burn(BurnDialog { armed: true, .. }) => &[Hint(Action::Confirm, tr!("Erase and burn")), Hint(Action::Back, tr!("Cancel"))],
                    _ => &[Hint(Action::Confirm, tr!("Burn")), Hint(Action::Back, tr!("Back"))],
                },
            },
            Mode::FontManager { armed: true, .. } => &[Hint(Action::Confirm, tr!("Delete")), Hint(Action::Back, tr!("Keep"))],
            Mode::FontManager { .. } => &[
                Hint(Action::Confirm, tr!("Select")),
                Hint(Action::Add, tr!("Import")),
                Hint(Action::Back, tr!("Back")),
            ],
        };
        let mut hints = hints.to_vec();
        if matches!(self.mode, Mode::Home) {
            if self.special_bar(Special::Add).is_some() {
                hints.retain(|h| h.0 != Action::Add);
            }
            hints.extend(self.bar_shortcuts(BarSlot::Bottom));
        }
        let y = self.bar_rect(BarSlot::Bottom, screen, s).center().y;
        self.draw_hint_row(ui, &hints, screen.right() - 60.0 * s, y, s, "hint")
    }

    fn draw_top_shortcuts(&self, ui: &Ui, screen: Rect, s: f32) -> Option<Action> {
        if !matches!(self.mode, Mode::Home) {
            return None;
        }
        let hints = self.bar_shortcuts(BarSlot::Top);
        let mut right = screen.right() - 160.0 * s;
        if self.global_mouse.connected() {
            let text = tr!("Global Virtual Mouse");
            right -= ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(18.0 * s), PAD_A).size().x + 48.0 * s;
        }
        let y = self.bar_rect(BarSlot::Top, screen, s).center().y;
        self.draw_hint_row(ui, &hints, right, y, s, "top-shortcut")
    }

    fn draw_hint_row(&self, ui: &Ui, hints: &[Hint], right: f32, y: f32, s: f32, id: &str) -> Option<Action> {
        let gamepad = self.input.has_gamepad();
        let font = FontId::proportional(20.0 * s);
        let h = 30.0 * s;
        let mut x = right;
        let painter = ui.painter();
        let mut clicked = None;
        for (i, Hint(action, label)) in hints.iter().enumerate().rev() {
            let control = action.control();
            let no_key = keyboard_key(*action).is_empty();
            let (glyph, skin, click) = match self.input.bindings().button_for(control).filter(|_| gamepad || no_key) {
                _ if matches!(control, Control::Up | Control::Down | Control::Left | Control::Right) => {
                    if gamepad { ("D-pad", ButtonSkin::plain(PAD_MENU), None) } else { (tr!("Arrows"), ButtonSkin::plain(KEY_FILL), None) }
                }
                Some(button) => (button.glyph(), self.button_skin(button.glyph()), Some(*action)),
                None => (keyboard_key(*action), ButtonSkin::plain(KEY_FILL), Some(*action)),
            };
            let label = painter.layout_no_wrap((*label).to_owned(), font.clone(), Color32::from_gray(220));
            let left = x - label.size().x - 10.0 * s - button_width(painter, glyph, &skin, &font, h, s);
            let area = Rect::from_min_max(pos2(left, y - h / 2.0), pos2(x, y + h / 2.0)).expand(6.0 * s);
            if let Some(action) = click {
                let resp = ui.interact(area, Id::new((id, i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.hovered() {
                    painter.rect_filled(area, (h / 2.0 + 6.0 * s) as u8, Color32::from_white_alpha(18));
                }
                if resp.clicked() {
                    clicked = Some(action);
                }
            }
            paint_button(painter, pos2(left, y), glyph, &skin, font.clone(), h, s);
            painter.galley(pos2(x - label.size().x, y - label.size().y / 2.0), label, Color32::from_gray(220));
            x = left - 28.0 * s;
        }
        clicked
    }

    fn draw_cursor(&self, ctx: &Context, s: f32) {
        let custom = self.settings.mouse.cursor_path();
        let pos = match self.input.virtual_pointer() {
            Some(pos) => pos,
            None if custom.is_some() => match ctx.pointer_hover_pos() {
                Some(pos) => pos,
                None => return,
            },
            None => return,
        };
        if custom.is_some() && ctx.output(|o| o.cursor_icon) == egui::CursorIcon::Text {
            return;
        }
        ctx.set_cursor_icon(egui::CursorIcon::None);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Debug, Id::new("cursor")));
        let h = self.settings.mouse.cursor_size * s.max(0.6);
        if let Some(path) = custom {
            let image = egui::Image::new(file_uri(&path));
            if let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(ctx, vec2(h, h)) {
                let w = h * texture.size.x / texture.size.y.max(1.0);
                let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
                painter.image(texture.id, Rect::from_min_size(pos, vec2(w, h)), uv, Color32::WHITE);
                return;
            }
        }
        let u = h / 20.0;
        let arrow = [(0.0, 0.0), (0.0, 17.0), (12.0, 12.0)].map(|(x, y)| pos + vec2(x, y) * u);
        painter.add(Shape::convex_polygon(arrow.to_vec(), Color32::WHITE, Stroke::new(1.5 * u.max(0.7), Color32::BLACK)));
    }

    fn draw_options(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let title = self.selected_game().map(|g| g.name.clone()).unwrap_or_default();
        let menu = self.menu();
        let Mode::Options { cursor } = &mut self.mode else { return };
        *cursor = (*cursor).min(menu.len() - 1);
        let rows: Vec<_> = menu.iter().map(|(_, label)| (label.as_str(), None)).collect();
        if let Some(i) = side_panel(ui, screen, s, &title, &rows, cursor) {
            self.run_menu_item(menu[i].0);
        }
    }

    fn draw_add_menu(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::AddMenu { cursor } = &mut self.mode else { return };
        let rows: Vec<_> = ADD_MENU.iter().map(|(_, label, sub)| (i18n::tr(label), Some(i18n::tr(sub)))).collect();
        if let Some(i) = side_panel(ui, screen, s, tr!("Add game"), &rows, cursor) {
            self.run_add_source(ADD_MENU[i].0);
        }
    }

    fn draw_picker(&mut self, ui: &Ui, screen: Rect, s: f32) {
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

    fn draw_scan(&mut self, ui: &Ui, screen: Rect, s: f32) {
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

    fn draw_confirm_remove(&mut self, ui: &Ui, screen: Rect, s: f32) {
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
                tr!("Its address and API key are forgotten. Apps and themes installed from it stay."),
            ),
            Removal::Addon(i) => {
                let Some(addon) = self.addons.installed.get(i) else { return };
                (tr!("Uninstall \"{name}\"?", name = addon.name), tr!("Its files are deleted. Its saved data is kept for a reinstall."))
            }
            Removal::Profile => (
                tr!("Reset profile \"{name}\"?", name = profiles::active()),
                tr!("Its library, settings, themes and apps are deleted. YASAFF restarts."),
            ),
            Removal::Everything => (
                tr!("Reset YASAFF?").to_owned(),
                tr!("Every profile and all data are deleted. YASAFF restarts as on a fresh install."),
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
            Removal::Theme => tr!("Delete"),
            Removal::AppStore => tr!("Remove"),
            Removal::Addon(_) => tr!("Uninstall"),
            Removal::Profile | Removal::Everything => tr!("Reset"),
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

    fn draw_cover_editor(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::CoverEditor(editor) = &self.mode else { return };
        let Some(game) = self.selected_game() else { return };
        let (kind, mut draft, mut cursor, mut button) = (editor.kind, editor.draft, editor.cursor, editor.button);
        let before = game.placement(kind);
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(190));
        let dialog = Rect::from_center_size(screen.center() - vec2(0.0, 20.0 * s), vec2((1100.0 * s).min(screen.width() - 40.0 * s), 820.0 * s));
        painter.rect_filled(dialog, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        let title = match kind {
            AssetKind::Spine => tr!("Spine image size and position · {name}", name = game.name),
            AssetKind::Top => tr!("Top image size and position · {name}", name = game.name),
            AssetKind::Back => tr!("Back cover size and position · {name}", name = game.name),
            AssetKind::Side => tr!("Side image size and position · {name}", name = game.name),
            AssetKind::Bottom => tr!("Bottom image size and position · {name}", name = game.name),
            AssetKind::Booklet => tr!("Booklet image size and position · {name}", name = game.name),
            _ => tr!("Cover size and position · {name}", name = game.name),
        };
        painter.text(dialog.left_top() + vec2(40.0 * s, 32.0 * s), Align2::LEFT_TOP, title, FontId::proportional(30.0 * s), Color32::WHITE);

        let settings = &self.theme().settings;
        let tile_h = 330.0 * s;
        let tile = vec2(tile_h * settings.tile_style.aspect(settings.boxes.size_mm()), tile_h);
        let changed = draft != before;
        for (n, (label, placement)) in [(tr!("Before"), before), (tr!("After"), draft)].into_iter().enumerate() {
            let center_x = dialog.center().x + (n as f32 - 0.5) * dialog.width() / 2.0;
            let color = if n == 1 && changed { ACCENT } else { Color32::from_gray(170) };
            painter.text(pos2(center_x, dialog.top() + 110.0 * s), Align2::CENTER_TOP, label, FontId::proportional(22.0 * s), color);
            let rect = Rect::from_center_size(pos2(center_x, dialog.top() + 150.0 * s + tile_h / 2.0), tile);
            *self.library.games[self.selected].placement_mut(kind) = placement;
            self.paint_tile_art(ui, self.selected, rect, (6.0 * s) as u8, s, 0.0);
        }
        *self.library.games[self.selected].placement_mut(kind) = before;

        let mut accept = None;
        let top = dialog.top() + 520.0 * s;
        let row_h = 62.0 * s;
        for (i, slider) in CoverSlider::ALL.into_iter().enumerate() {
            let r = Rect::from_min_size(pos2(dialog.left() + 20.0 * s, top + i as f32 * row_h), vec2(dialog.width() - 40.0 * s, row_h - 6.0 * s));
            let active = i == cursor;
            if active {
                painter.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(22));
                painter.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
            }
            let text_color = if active { Color32::WHITE } else { Color32::from_gray(190) };
            painter.text(pos2(r.left() + 30.0 * s, r.center().y), Align2::LEFT_CENTER, slider.label(), FontId::proportional(21.0 * s), text_color);
            let value_right = r.right() - 30.0 * s;
            painter.text(pos2(value_right, r.center().y), Align2::RIGHT_CENTER, slider.value(&draft), FontId::proportional(19.0 * s), text_color);
            let track = Rect::from_min_max(pos2(r.left() + 300.0 * s, r.center().y - 3.0 * s), pos2(value_right - 120.0 * s, r.center().y + 3.0 * s));
            let t = slider.get(&draft).clamp(0.0, 1.0);
            let knob = pos2(track.left() + track.width() * t, track.center().y);
            let saved_x = track.left() + track.width() * slider.get(&before).clamp(0.0, 1.0);
            painter.rect_filled(track, (3.0 * s) as u8, Color32::from_white_alpha(40));
            painter.rect_filled(Rect::from_min_max(track.min, pos2(knob.x, track.max.y)), (3.0 * s) as u8, ACCENT);
            painter.line_segment([pos2(saved_x, track.top() - 9.0 * s), pos2(saved_x, track.bottom() + 9.0 * s)], Stroke::new(2.0 * s, Color32::from_gray(150)));
            painter.circle_filled(knob, 11.0 * s, Color32::WHITE);

            let resp = ui.interact(r, Id::new(("cover-editor-row", i)), Sense::hover());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                cursor = i;
            }
            let drag = ui.interact(track.expand2(vec2(14.0 * s, 20.0 * s)), Id::new(("cover-editor-slider", i)), Sense::click_and_drag());
            if let Some(pos) = drag.interact_pointer_pos().filter(|_| drag.dragged() || drag.clicked()) {
                slider.set(&mut draft, (pos.x - track.left()) / track.width());
                cursor = i;
            }
        }

        let buttons_y = dialog.bottom() - 60.0 * s;
        let on_buttons = cursor == CoverSlider::ALL.len();
        for (n, b) in EditorButton::ALL.into_iter().enumerate() {
            let center = pos2(dialog.center().x + (n as f32 - 1.0) * 230.0 * s, buttons_y);
            let btn = Rect::from_center_size(center, vec2(200.0 * s, 54.0 * s));
            let focused = on_buttons && button == b;
            let fill = match (focused, b) {
                (true, _) => Color32::WHITE,
                (false, EditorButton::Accept) => ACCENT,
                (false, _) => Color32::from_white_alpha(25),
            };
            painter.rect_filled(btn, (4.0 * s) as u8, fill);
            let text = if focused { Color32::BLACK } else { Color32::WHITE };
            painter.text(btn.center(), Align2::CENTER_CENTER, b.label(), FontId::proportional(22.0 * s), text);
            let resp = ui.interact(btn, Id::new(("cover-editor-button", n)), Sense::click());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                (cursor, button) = (CoverSlider::ALL.len(), b);
            }
            if resp.clicked() {
                match b {
                    EditorButton::Reset => draft = CoverPlacement::default(),
                    EditorButton::Cancel => accept = Some(false),
                    EditorButton::Accept => accept = Some(true),
                }
            }
        }

        self.mode = Mode::CoverEditor(CoverEditor { kind, draft, cursor, button });
        if let Some(accept) = accept {
            self.close_cover_editor(accept);
        }
    }

    fn draw_prompt(&mut self, ctx: &Context) {
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
                tr!("YASAFF Store address"),
                Some(tr!("Paste the address shown on the store's API keys page. Just the site address works too.")),
            ),
            PromptKind::AppStoreKey => (
                tr!("YASAFF Store API key"),
                Some(tr!("Paste a key from the store's API keys page. Save it empty to disconnect.")),
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

    fn setting_heading(&self, row: SettingRow) -> Option<(&'static str, Option<(String, Color32)>)> {
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

    fn addons_heading(&self) -> (&'static str, Option<(String, Color32)>) {
        (tr!("Addons"), Some((tr!("{count} installed", count = self.addons.installed.len()), Color32::from_gray(140))))
    }

    fn buttons_heading(&self) -> (&'static str, Option<(String, Color32)>) {
        (tr!("Buttons"), Some((tr!("How A, B, X and Y look in the button hints").to_owned(), Color32::from_gray(140))))
    }

    fn setting_sections(&self, rows: &[SettingRow]) -> Vec<SettingSection> {
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

    fn setting_row_content(&self, row: SettingRow) -> (String, &'static str, RowValue) {
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
            SettingRow::ProfileExport => (tr!("Export current profile…").into(), tr!("Library, settings, themes, images and apps in one .zip"), Action(tr!("Export"))),
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
                tr!("Bubble sounds").into(),
                tr!("A soft bloop when moving between bubbles"),
                Toggle(theme.bubbles.sound),
            ),
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
                let sub = if has(|g| g.cover.is_some()) { tr!("The front of the box") } else { tr!("None; the name's initials are shown") };
                (tr!("Cover image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameSpine => {
                let sub = if has(|g| g.spine.is_some()) {
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
                let sub = if has(|g| g.top.is_some()) { tr!("Lies across the top of the box") } else { tr!("None; uses the top color") };
                (tr!("Top image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameTopClear => (tr!("Remove top image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameBack => {
                let sub = if has(|g| g.back.is_some()) { tr!("Seen while a 3D model box turns") } else { tr!("None; uses the spine color") };
                (tr!("Back cover image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameBackClear => (tr!("Remove back cover image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameSide => {
                let sub = if has(|g| g.side.is_some()) {
                    tr!("The edge opposite the spine; seen while a 3D model box turns")
                } else {
                    tr!("None; uses the top color")
                };
                (tr!("Right side image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameSideClear => (tr!("Remove right side image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameBottom => {
                let sub = if has(|g| g.bottom.is_some()) { tr!("Under the box; seen while editing it") } else { tr!("None; uses the top color") };
                (tr!("Bottom image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameBottomClear => (tr!("Remove bottom image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameBooklet => {
                let sub = if has(|g| g.booklet.is_some()) {
                    tr!("Inside the cover of a 3D opening case")
                } else {
                    tr!("None; the booklet shows the cover")
                };
                (tr!("Instruction booklet image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameBookletClear => (tr!("Remove instruction booklet image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameModel => {
                let sub = if has(|g| g.model.is_some()) {
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
                let sub = if has(|g| g.video.is_some()) { tr!("Copied next to the game's images") } else { tr!("None; the cover is shown") };
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

    fn draw_settings(&mut self, ui: &Ui, screen: Rect, s: f32) {
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

    fn draw_font_manager(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::FontManager { cursor, armed } = self.mode else { return };
        let count = self.fonts.len() + 1;
        let cursor = cursor.min(count - 1);
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(80));
        let panel = Rect::from_min_max(screen.min, pos2(screen.left() + (640.0 * s).min(screen.width()), screen.bottom()));
        painter.rect_filled(panel, 0, Color32::from_rgba_unmultiplied(10, 16, 32, 240));
        let x = panel.left() + 40.0 * s;
        painter.text(pos2(x, panel.top() + 110.0 * s), Align2::LEFT_TOP, tr!("Fonts"), FontId::proportional(30.0 * s), Color32::WHITE);
        let note = tr!("{count} installed · delete to remove it from every game", count = self.fonts.len());
        painter.text(pos2(x, panel.top() + 150.0 * s), Align2::LEFT_TOP, note, FontId::proportional(16.0 * s), Color32::from_gray(140));

        let row_h = 84.0 * s;
        let step = row_h + 6.0 * s;
        let top = panel.top() + 200.0 * s;
        let fits = (((panel.bottom() - 90.0 * s - top) / step).floor() as usize).max(1);
        let first = (cursor + 1).saturating_sub(fits);
        let mut new_cursor = cursor;
        let mut clicked = false;
        for (slot, i) in (first..count.min(first + fits)).enumerate() {
            let row = Rect::from_min_size(pos2(panel.left(), top + slot as f32 * step), vec2(panel.width(), row_h));
            let resp = ui.interact(row, Id::new(("font-row", i)), Sense::click());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                new_cursor = i;
            }
            if resp.clicked() {
                (new_cursor, clicked) = (i, true);
            }
            let active = i == cursor;
            if active {
                painter.rect_filled(row, 0, Color32::from_white_alpha(22));
                painter.rect_filled(Rect::from_min_size(row.min, vec2(5.0 * s, row.height())), 0, ACCENT);
            }
            let color = if active { Color32::WHITE } else { Color32::from_gray(180) };
            let right = row.right() - 40.0 * s;
            let Some(file) = self.fonts.get(i) else {
                painter.text(pos2(x, row.center().y - 4.0 * s), Align2::LEFT_BOTTOM, tr!("Import fonts…"), FontId::proportional(24.0 * s), color);
                painter.text(pos2(x, row.center().y + 4.0 * s), Align2::LEFT_TOP, tr!(".ttf, .otf or .ttc files"), FontId::proportional(16.0 * s), Color32::from_gray(140));
                continue;
            };
            let action = match (active, armed) {
                (true, true) => Some((tr!("Press again to delete"), PAD_B)),
                (true, false) => Some((tr!("Delete"), Color32::from_gray(200))),
                _ => None,
            };
            let mut text_right = right;
            if let Some((text, c)) = action {
                text_right = painter.text(pos2(right, row.center().y), Align2::RIGHT_CENTER, text, FontId::proportional(17.0 * s), c).left() - 20.0 * s;
            }
            let clip = painter.with_clip_rect(Rect::from_min_max(row.min, pos2(text_right, row.max.y)));
            clip.text(pos2(x, row.center().y - 4.0 * s), Align2::LEFT_BOTTOM, fonts::display_name(file), fonts::of(file, 26.0 * s), color);
            clip.text(pos2(x, row.center().y + 4.0 * s), Align2::LEFT_TOP, self.font_usage(file), FontId::proportional(16.0 * s), Color32::from_gray(140));
        }
        if let Mode::FontManager { cursor, armed } = &mut self.mode
            && new_cursor != *cursor
        {
            (*cursor, *armed) = (new_cursor, false);
        }
        if clicked {
            self.activate_font_row();
        }
    }

    fn draw_launching(&self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::Launching(l) = &self.mode else { return };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(215));
        let tile = Rect::from_center_size(screen.center() - vec2(0.0, 120.0 * s), vec2(260.0 * s, 260.0 * s));
        let radius = (6.0 * s) as u8;
        match &l.cover {
            Some(cover) => paint_image_cover(ui, cover, tile, 1.0, radius),
            None => {
                painter.rect_filled(tile, radius, Color32::from_white_alpha(25));
                let initial = l.name.chars().next().unwrap_or('?').to_uppercase().to_string();
                painter.text(tile.center(), Align2::CENTER_CENTER, initial, FontId::proportional(90.0 * s), Color32::WHITE);
            }
        }
        let x = screen.center().x;
        let mut y = tile.bottom() + 40.0 * s;
        painter.text(pos2(x, y), Align2::CENTER_TOP, tr!("Starting"), FontId::proportional(20.0 * s), Color32::from_gray(170));
        y += 32.0 * s;
        painter.text(pos2(x, y), Align2::CENTER_TOP, &l.name, FontId::new(42.0 * s, l.font.clone()), Color32::WHITE);
        y += 80.0 * s;
        egui::Spinner::new().size(40.0 * s).color(Color32::WHITE).paint_at(ui, Rect::from_center_size(pos2(x, y), vec2(40.0 * s, 40.0 * s)));
        y += 44.0 * s;
        let secs = l.started.elapsed().as_secs();
        let status = match l.via {
            Some(store) => tr!("via {store} · {secs} s", store = store.name(), secs = secs),
            None => format!("{secs} s"),
        };
        painter.text(pos2(x, y), Align2::CENTER_TOP, status, FontId::proportional(18.0 * s), Color32::from_gray(150));
        if secs >= 10 {
            painter.text(pos2(x, y + 30.0 * s), Align2::CENTER_TOP, tr!("Taking a while? Press B to hide this screen; the game keeps starting."), FontId::proportional(16.0 * s), Color32::from_gray(120));
        }
    }

    fn draw_disc_chooser(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::DiscChooser { name, candidates, cursor } = &self.mode else { return };
        let (cursor, count) = (*cursor, candidates.len());
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(190));

        let gap = 30.0 * s;
        let tile = (240.0 * s).min((screen.width() - 120.0 * s - gap * (count as f32 - 1.0)) / count as f32);
        let row_w = tile * count as f32 + gap * (count as f32 - 1.0);
        let dialog = Rect::from_center_size(screen.center(), vec2((row_w + 120.0 * s).max(640.0 * s), tile + 290.0 * s));
        painter.rect_filled(dialog, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        let x = dialog.center().x;
        painter.text(pos2(x, dialog.top() + 36.0 * s), Align2::CENTER_TOP, tr!("Disc inserted: {name}", name = name), FontId::proportional(20.0 * s), Color32::from_gray(170));
        let title = if count == 1 { tr!("Start this game?") } else { tr!("Several games match. Which one?") };
        painter.text(pos2(x, dialog.top() + 66.0 * s), Align2::CENTER_TOP, title, FontId::proportional(30.0 * s), Color32::WHITE);

        let mut clicked = None;
        let top = dialog.top() + 130.0 * s;
        for (slot, &index) in candidates.iter().enumerate() {
            let Some(game) = self.library.games.get(index) else { continue };
            let rect = Rect::from_min_size(pos2(x - row_w / 2.0 + slot as f32 * (tile + gap), top), vec2(tile, tile));
            draw_game_tile(ui, game, rect, (6.0 * s) as u8);
            if slot == cursor {
                painter.rect_stroke(rect.expand(4.0 * s), (8.0 * s) as u8, Stroke::new(3.0 * s, Color32::WHITE), StrokeKind::Outside);
            }
            let color = if slot == cursor { Color32::WHITE } else { Color32::from_gray(170) };
            let label = painter.layout(game.name.clone(), self.game_title_font(game, 20.0 * s), color, tile + gap);
            painter.galley(pos2(rect.center().x - label.size().x / 2.0, rect.bottom() + 14.0 * s), label, color);
            let via = Store::for_command(&game.target, &game.args).map(|(st, _)| st.short_name()).unwrap_or("");
            painter.text(pos2(rect.center().x, rect.bottom() + 70.0 * s), Align2::CENTER_TOP, via, FontId::proportional(16.0 * s), Color32::from_gray(130));
            let resp = ui.interact(rect, Id::new(("disc-choice", slot)), Sense::click());
            if resp.clicked() {
                clicked = Some(slot);
            }
        }
        if let Some(slot) = clicked
            && let Mode::DiscChooser { cursor, .. } = &mut self.mode
        {
            *cursor = slot;
            self.handle(ui.ctx(), Action::Confirm);
        }
    }

    fn draw_welcome(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::Welcome { cursor } = self.mode else { return };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(170));
        let button_h = 88.0 * s;
        let gap = 14.0 * s;
        let height = 170.0 * s + WELCOME.len() as f32 * (button_h + gap);
        let dialog = Rect::from_center_size(screen.center(), vec2((760.0 * s).min(screen.width() - 40.0), height));
        painter.rect_filled(dialog, (8.0 * s) as u8, Color32::from_rgb(14, 20, 36));
        painter.text(dialog.center_top() + vec2(0.0, 44.0 * s), Align2::CENTER_TOP, tr!("Welcome!"), FontId::proportional(32.0 * s), Color32::WHITE);
        let subtitle = tr!("How would you like to start?");
        painter.text(dialog.center_top() + vec2(0.0, 92.0 * s), Align2::CENTER_TOP, subtitle, FontId::proportional(19.0 * s), Color32::from_gray(170));

        let mut hovered = None;
        let mut clicked = None;
        for (i, (_, label, note)) in WELCOME.iter().enumerate() {
            let top = dialog.top() + 146.0 * s + i as f32 * (button_h + gap);
            let r = Rect::from_min_size(pos2(dialog.left() + 40.0 * s, top), vec2(dialog.width() - 80.0 * s, button_h));
            let focused = i == cursor;
            let (fill, text, sub) = if focused {
                (Color32::WHITE, Color32::BLACK, Color32::from_gray(70))
            } else {
                (Color32::from_white_alpha(22), Color32::WHITE, Color32::from_gray(150))
            };
            painter.rect_filled(r, (6.0 * s) as u8, fill);
            let x = r.left() + 28.0 * s;
            painter.text(pos2(x, r.center().y - 4.0 * s), Align2::LEFT_BOTTOM, i18n::tr(label), FontId::proportional(24.0 * s), text);
            painter.text(pos2(x, r.center().y + 2.0 * s), Align2::LEFT_TOP, i18n::tr(note), FontId::proportional(16.0 * s), sub);
            let resp = ui.interact(r, Id::new(("welcome", i)), Sense::click());
            if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                hovered = Some(i);
            }
            if resp.clicked() {
                clicked = Some(i);
            }
        }
        if let Some(i) = hovered {
            self.mode = Mode::Welcome { cursor: i };
        }
        if let Some(i) = clicked {
            self.start_with(WELCOME[i].0);
        }
    }

    fn draw_disc_not_found(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::DiscNotFound { name } = &self.mode else { return };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(160));
        let dialog = Rect::from_center_size(screen.center(), vec2(720.0 * s, 260.0 * s));
        painter.rect_filled(dialog, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        let title = painter.layout(tr!("\"{name}\" not found", name = name), FontId::proportional(28.0 * s), Color32::WHITE, dialog.width() - 60.0 * s);
        painter.galley(pos2(dialog.center().x - title.size().x / 2.0, dialog.top() + 36.0 * s), title, Color32::WHITE);
        painter.text(
            dialog.center_top() + vec2(0.0, 110.0 * s),
            Align2::CENTER_CENTER,
            tr!("This disc's game isn't in your library. Add it, then insert the disc again."),
            FontId::proportional(17.0 * s),
            Color32::from_gray(170),
        );
        let btn = Rect::from_center_size(dialog.center_bottom() - vec2(0.0, 60.0 * s), vec2(200.0 * s, 54.0 * s));
        painter.rect_filled(btn, (4.0 * s) as u8, Color32::WHITE);
        painter.text(btn.center(), Align2::CENTER_CENTER, tr!("OK"), FontId::proportional(22.0 * s), Color32::BLACK);
        if ui.interact(btn, Id::new("disc-not-found-ok"), Sense::click()).clicked() {
            self.mode = Mode::Home;
        }
    }

    fn draw_profile_imported(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::ProfileImported { name, missing, switch } = &self.mode else { return };
        const SHOWN: usize = 12;
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(160));
        let lines = missing.len().min(SHOWN + 1);
        let height = (260.0 + if missing.is_empty() { 0.0 } else { 40.0 + 26.0 * lines as f32 }) * s;
        let dialog = Rect::from_center_size(screen.center(), vec2((900.0 * s).min(screen.width() - 40.0), height.min(screen.height() - 40.0)));
        painter.rect_filled(dialog, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        painter.text(
            dialog.center_top() + vec2(0.0, 50.0 * s),
            Align2::CENTER_CENTER,
            tr!("Imported as \"{name}\"", name = name),
            FontId::proportional(28.0 * s),
            Color32::WHITE,
        );
        let gray = Color32::from_gray(170);
        let detail = if missing.is_empty() {
            tr!("Everything it uses was found on this computer.")
        } else {
            tr!("Not found on this computer; fix these after switching:")
        };
        painter.text(dialog.center_top() + vec2(0.0, 95.0 * s), Align2::CENTER_CENTER, detail, FontId::proportional(17.0 * s), gray);
        let left = dialog.left() + 40.0 * s;
        let width = dialog.width() - 80.0 * s;
        for (i, line) in missing.iter().take(SHOWN).enumerate() {
            let galley = painter.layout_no_wrap(format!("• {line}"), FontId::proportional(16.0 * s), PAD_Y);
            let pos = pos2(left, dialog.top() + (125.0 + 26.0 * i as f32) * s);
            painter.with_clip_rect(Rect::from_min_size(pos, vec2(width, 26.0 * s))).galley(pos, galley, PAD_Y);
        }
        if missing.len() > SHOWN {
            let more = tr!("…and {count} more", count = missing.len() - SHOWN);
            painter.text(pos2(left, dialog.top() + (125.0 + 26.0 * SHOWN as f32) * s), Align2::LEFT_TOP, more, FontId::proportional(16.0 * s), gray);
        }
        let switch_label = tr!("Switch to it");
        let mut choice = None;
        for (i, (label, is_switch)) in [(tr!("Stay"), false), (switch_label, true)].into_iter().enumerate() {
            let center = dialog.center_bottom() + vec2((i as f32 - 0.5) * 240.0 * s, -60.0 * s);
            let btn = Rect::from_center_size(center, vec2(200.0 * s, 54.0 * s));
            let focused = *switch == is_switch;
            painter.rect_filled(btn, (4.0 * s) as u8, if focused { Color32::WHITE } else { Color32::from_white_alpha(25) });
            painter.text(btn.center(), Align2::CENTER_CENTER, label, FontId::proportional(22.0 * s), if focused { Color32::BLACK } else { Color32::WHITE });
            if ui.interact(btn, Id::new(("profile-imported", i)), Sense::click()).clicked() {
                choice = Some(is_switch);
            }
        }
        if let Some(pick) = choice
            && let Mode::ProfileImported { switch, .. } = &mut self.mode
        {
            *switch = pick;
            self.handle(ui.ctx(), Action::Confirm);
        }
    }

    fn draw_burn(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::Burn(dialog) = &self.mode else { return };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(190));
        let panel = Rect::from_center_size(screen.center(), vec2((900.0 * s).min(screen.width() - 40.0), 560.0 * s));
        painter.rect_filled(panel, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        let left = panel.left() + 40.0 * s;
        let gray = Color32::from_gray(170);

        if let Some(b) = &self.burning {
            painter.text(pos2(left, panel.top() + 36.0 * s), Align2::LEFT_TOP, tr!("Burning {game}", game = b.game), FontId::proportional(30.0 * s), Color32::WHITE);
            painter.text(pos2(left, panel.top() + 84.0 * s), Align2::LEFT_TOP, &b.drive, FontId::proportional(18.0 * s), gray);

            let bar = Rect::from_min_size(pos2(left, panel.top() + 140.0 * s), vec2(panel.width() - 80.0 * s, 28.0 * s));
            let radius = (bar.height() / 2.0) as u8;
            painter.rect_filled(bar, radius, Color32::from_white_alpha(25));
            let (fill, color) = match &b.result {
                Some(Ok(())) => (Some(1.0), Color32::from_rgb(0x5c, 0xb8, 0x3c)),
                Some(Err(_)) => (Some(1.0), PAD_B),
                None => (b.progress.fraction, ACCENT),
            };
            match fill {
                Some(f) => {
                    let w = (bar.width() * f).max(bar.height());
                    painter.rect_filled(Rect::from_min_size(bar.min, vec2(w, bar.height())), radius, color);
                }
                None => {
                    let t = ui.input(|i| i.time) as f32;
                    let w = bar.width() * 0.25;
                    let x = bar.left() + (bar.width() - w) * (0.5 - 0.5 * (t * 2.0).cos());
                    painter.rect_filled(Rect::from_min_size(pos2(x, bar.top()), vec2(w, bar.height())), radius, color);
                    ui.ctx().request_repaint();
                }
            }

            let (status, detail) = match &b.result {
                None => (b.progress.status.clone(), tr!("Don't open the tray. B hides this screen; burning carries on.").to_owned()),
                Some(Ok(())) => (tr!("Disc burned").to_owned(), tr!("Take it out; inserting it again starts the game.").to_owned()),
                Some(Err(e)) => (tr!("Burning failed").to_owned(), e.clone()),
            };
            let btn = Rect::from_center_size(panel.center_bottom() - vec2(0.0, 50.0 * s), vec2(200.0 * s, 54.0 * s));
            painter.text(pos2(left, bar.bottom() + 20.0 * s), Align2::LEFT_TOP, status, FontId::proportional(24.0 * s), Color32::WHITE);
            let detail = painter.layout(detail, FontId::proportional(16.0 * s), gray, bar.width());
            let clip = Rect::from_x_y_ranges(panel.x_range(), bar.bottom() + 56.0 * s..=btn.top() - 12.0 * s);
            painter.with_clip_rect(clip).galley(pos2(left, clip.top()), detail, gray);
            if b.result.is_some() {
                let label = if matches!(b.result, Some(Err(_))) { tr!("Try again") } else { tr!("OK") };
                painter.rect_filled(btn, (4.0 * s) as u8, Color32::WHITE);
                painter.text(btn.center(), Align2::CENTER_CENTER, label, FontId::proportional(22.0 * s), Color32::BLACK);
                if ui.interact(btn, Id::new("burn-ok"), Sense::click()).clicked() {
                    self.close_burner();
                }
            }
            return;
        }

        let game = self.selected_game().map(|g| g.name.clone()).unwrap_or_default();
        painter.text(pos2(left, panel.top() + 36.0 * s), Align2::LEFT_TOP, tr!("Burn to CD/DVD"), FontId::proportional(30.0 * s), Color32::WHITE);
        painter.text(pos2(left, panel.top() + 84.0 * s), Align2::LEFT_TOP, tr!("Disc file for {game}", game = game), FontId::proportional(18.0 * s), gray);

        let top = panel.top() + 130.0 * s;
        if dialog.burners.is_empty() {
            let text = tr!("No CD/DVD burner found. Connect one (USB burners work too); it shows up here.");
            painter.text(pos2(panel.center().x, top + 80.0 * s), Align2::CENTER_CENTER, text, FontId::proportional(19.0 * s), Color32::from_gray(150));
        }
        let row_h = 82.0 * s;
        let rows = ((panel.bottom() - 90.0 * s - top) / row_h).floor().max(1.0) as usize;
        let first = dialog.cursor.saturating_sub(rows - 1);
        let mut clicked = None;
        for (i, burner) in dialog.burners.iter().enumerate().skip(first).take(rows) {
            let r = Rect::from_min_size(pos2(panel.left() + 20.0 * s, top + (i - first) as f32 * row_h), vec2(panel.width() - 40.0 * s, row_h - 8.0 * s));
            if i == dialog.cursor {
                painter.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(22));
                painter.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
            }
            let (tag, tag_color) = match &burner.media {
                m @ burn::Media::Blank(_) => (tr!("{disc} · ready", disc = m), Color32::from_rgb(0x5c, 0xb8, 0x3c)),
                m @ burn::Media::Rewritable(_) if i == dialog.cursor && dialog.armed => {
                    (tr!("{disc} · press again to erase it", disc = m), PAD_B)
                }
                m @ burn::Media::Rewritable(_) => (tr!("{disc} · will be erased", disc = m), PAD_Y),
                m => (m.to_string(), Color32::from_gray(110)),
            };
            let tag = painter.layout_no_wrap(tag, FontId::proportional(16.0 * s), Color32::WHITE);
            let t = Rect::from_min_size(pos2(r.right() - 20.0 * s - tag.size().x - 24.0 * s, r.center().y - tag.size().y / 2.0 - 6.0 * s), tag.size() + vec2(24.0 * s, 12.0 * s));
            painter.rect_filled(t, (t.height() / 2.0) as u8, tag_color.gamma_multiply(0.45));
            painter.galley(t.center() - tag.size() / 2.0, tag, Color32::WHITE);

            let clip = painter.with_clip_rect(Rect::from_min_max(r.min, pos2(t.left() - 12.0 * s, r.bottom())));
            clip.text(pos2(r.left() + 26.0 * s, r.top() + 10.0 * s), Align2::LEFT_TOP, &burner.name, FontId::proportional(22.0 * s), Color32::WHITE);
            let info = tr!("{device} · writes {kinds}", device = burner.device.display(), kinds = burner.writes.join(", "));
            clip.text(pos2(r.left() + 26.0 * s, r.top() + 42.0 * s), Align2::LEFT_TOP, info, FontId::proportional(15.0 * s), Color32::from_gray(140));
            if ui.interact(r, Id::new(("burner", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }
        let note = tr!("Writes one small .yasaff file, so any blank disc will do. The disc is ejected when done.");
        painter.text(pos2(left, panel.bottom() - 40.0 * s), Align2::LEFT_CENTER, note, FontId::proportional(15.0 * s), Color32::from_gray(130));

        if let Some(i) = clicked
            && let Mode::Burn(dialog) = &mut self.mode
        {
            if dialog.cursor != i {
                (dialog.cursor, dialog.armed) = (i, false);
            }
            self.handle(ui.ctx(), Action::Confirm);
        }
    }

    fn draw_toast(&mut self, painter: &Painter, screen: Rect, s: f32) {
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

impl eframe::App for App {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().minimized) != Some(true) {
            return;
        }
        for action in self.input.poll_hidden() {
            self.handle(ctx, action);
        }
        self.poll_running(ctx);
        self.poll_game(ctx);
        self.poll_store();
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn raw_input_hook(&mut self, _ctx: &Context, raw_input: &mut egui::RawInput) {
        self.input.set_global_mouse(self.global_mouse.connected());
        self.input.feed_virtual_mouse(raw_input, self.settings.mouse.speed);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !matches!(self.mode, Mode::Prompt { .. })
            && let Some(id) = ctx.memory(|m| m.focused())
        {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        self.objects.get_mut().collect_garbage();
        let capture_text = matches!(self.mode, Mode::Picker(_));
        for action in self.input.poll(&ctx, capture_text) {
            self.handle(&ctx, action);
        }
        if ctx.input(|i| i.focused) {
            self.last_focused = Instant::now();
        }
        if let Some(loaded) = self.fonts_pending.take() {
            self.fonts = loaded;
        }
        self.poll_discs(&ctx);
        self.poll_scan();
        self.poll_burn();
        self.poll_artwork();
        self.poll_store();
        self.poll_running(&ctx);
        self.poll_game(&ctx);
        self.poll_save_dirs();
        self.poll_backup();
        self.poll_profile_job();
        self.poll_setup();
        self.update_launching(&ctx);
        if !matches!(self.mode, Mode::Home) {
            self.bubble_empty = None;
        }
        self.update_video(&ctx);
        self.guard_focus(&ctx);

        let screen = ui.max_rect();
        let s = (screen.height() / 1080.0).max(0.45);

        self.addons.sync(&self.settings);
        self.draw_background(ui, screen);
        self.draw_bars(ui, screen, s);
        let painter = ui.painter().clone();
        self.draw_top_bar(&painter, screen, s);
        self.draw_widgets(&painter, screen, s);
        match self.theme().settings.view {
            _ if self.is_bubbles() => self.draw_bubbles(ui, screen, s),
            ViewKind::Row => self.draw_row(ui, screen, s),
            ViewKind::Grid => self.draw_grid(ui, screen, s),
        }
        self.draw_options(ui, screen, s);
        self.draw_add_menu(ui, screen, s);
        self.draw_picker(ui, screen, s);
        self.draw_scan(ui, screen, s);
        self.draw_welcome(ui, screen, s);
        self.draw_store(ui, screen, s);
        self.draw_store_theme(ui, screen, s);
        self.draw_confirm_remove(ui, screen, s);
        self.draw_cover_editor(ui, screen, s);
        if let Some(action) = self.draw_hints(ui, screen, s).or(self.draw_top_shortcuts(ui, screen, s)) {
            self.handle(&ctx, action);
        }
        self.draw_settings(ui, screen, s);
        self.draw_font_manager(ui, screen, s);
        self.draw_disc_chooser(ui, screen, s);
        self.draw_disc_not_found(ui, screen, s);
        self.draw_profile_imported(ui, screen, s);
        self.draw_burn(ui, screen, s);
        self.draw_setup(&painter, screen, s);
        self.draw_toast(&painter, screen, s);
        self.draw_launching(ui, screen, s);
        self.draw_prompt(&ctx);
        self.draw_cursor(&ctx, s);

        let focused = ctx.input(|i| i.focused);
        ctx.request_repaint_after(Duration::from_millis(if focused { 16 } else { 100 }));
    }
}

fn disc_from_path(path: &Path) -> anyhow::Result<DiscEvent> {
    let (root, manifests) = if path.is_dir() {
        (path.to_path_buf(), crate::physical::manifest::read_all(path)?)
    } else {
        let parent = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        (parent, vec![(path.to_path_buf(), DiscManifest::read(path))])
    };
    anyhow::ensure!(!manifests.is_empty(), "no .yasaff file in that folder");
    let label = root.file_name().map(|n| n.to_string_lossy().into_owned());
    Ok(DiscEvent { volume: Volume { root, kind: VolumeKind::Optical, label }, manifests })
}

fn disc_manifest(game: &Game) -> DiscManifest {
    DiscManifest { id: Some(game.id.clone()), ids: game.external_ids(), name: Some(game.name.clone()) }
}

fn disc_file_name(game: &Game) -> String {
    let name: String = game.name.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect();
    format!("{}.yasaff", name.trim())
}

fn volume_name(volume: &Volume) -> String {
    let name = match &volume.label {
        Some(label) => label.clone(),
        None => volume.root.display().to_string(),
    };
    match volume.kind {
        VolumeKind::Optical => tr!("disc {name}", name = name),
        VolumeKind::Removable => tr!("drive {name}", name = name),
    }
}

const COMMAND_HELP: &str = tr_key!("Examples:  steam://rungameid/570   ·   wine \"/games/TQ/TQ.exe\"   ·   flatpak run com.example.Game\n\
     Quote arguments that contain spaces. URIs are opened by the system handler.");

fn step(value: f32, delta: f32, (min, max): (f32, f32)) -> f32 {
    (value + delta).clamp(min, max)
}

fn snap(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn position_label(t: f32, start: &str, end: &str) -> String {
    match (t * 100.0).round() as i32 {
        0 => start.to_owned(),
        50 => tr!("Center").to_owned(),
        100 => end.to_owned(),
        p => format!("{p} %"),
    }
}

fn step_count(value: u32, delta: i32, (min, max): (u32, u32)) -> u32 {
    value.saturating_add_signed(delta).clamp(min, max)
}

#[derive(Clone, Copy)]
enum Layout {
    Row { rows: usize },
    Grid { columns: usize, rows: usize },
}

fn navigate(layout: Layout, selected: usize, count: usize, action: Action) -> usize {
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

fn navigate_free(centers: &[Pos2], selected: usize, action: Action) -> usize {
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

struct BubbleLayout {
    diameters: Vec<f32>,
    area: Rect,
    centers: Vec<Pos2>,
    radii: Vec<f32>,
}

fn pack_bubbles(diameters: &[f32], area: Rect) -> (Vec<Pos2>, Vec<f32>) {
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

const SWAP_GLOW: Color32 = Color32::from_rgb(255, 210, 60);
const RUNNING_GLOW: Color32 = Color32::from_rgb(70, 230, 90);

struct BubbleLook {
    empty: bool,
    texture: Option<(TextureId, Vec2)>,
    fill: Color32,
    label: String,
    swapping: bool,
    running: bool,
}

fn disc_mesh(texture: TextureId, rings: usize, segments: usize, vertex: impl Fn(f32, f32) -> (Pos2, Pos2, Color32)) -> Mesh {
    let mut mesh = Mesh::with_texture(texture);
    let push = |mesh: &mut Mesh, rho: f32, theta: f32| {
        let (pos, uv, color) = vertex(rho, theta);
        mesh.vertices.push(egui::epaint::Vertex { pos, uv, color });
    };
    push(&mut mesh, 0.0, 0.0);
    for ring in 1..=rings {
        let rho = ring as f32 / rings as f32;
        for seg in 0..segments {
            push(&mut mesh, rho, seg as f32 / segments as f32 * std::f32::consts::TAU);
        }
    }
    let at = |ring: usize, seg: usize| (1 + (ring - 1) * segments + seg % segments) as u32;
    for seg in 0..segments {
        mesh.add_triangle(0, at(1, seg), at(1, seg + 1));
        for ring in 1..rings {
            mesh.add_triangle(at(ring, seg), at(ring + 1, seg), at(ring + 1, seg + 1));
            mesh.add_triangle(at(ring, seg), at(ring + 1, seg + 1), at(ring, seg + 1));
        }
    }
    mesh
}

fn scale_color(c: Color32, k: f32) -> Color32 {
    let f = |v: u8| (v as f32 * k).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_premultiplied(f(c.r()), f(c.g()), f(c.b()), c.a())
}

fn paint_bubble(painter: &Painter, center: Pos2, r: f32, look: &BubbleLook, t: f32, focus: f32, s: f32) {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let white_uv = egui::epaint::WHITE_UV;
    if !look.empty {
        painter.circle_filled(center + vec2(0.0, r * 0.06 + 6.0 * s), r * 0.97, Color32::from_black_alpha(60));
    }
    let running_pulse = 0.5 + 0.5 * (t * 3.0).sin();
    if look.running {
        let reach = (r * 0.35).clamp(14.0 * s, 40.0 * s) * (0.8 + 0.4 * running_pulse);
        const RINGS: usize = 12;
        let step = reach / RINGS as f32;
        for k in 0..RINGS {
            let fade = (1.0 - k as f32 / RINGS as f32).powi(2);
            let glow = RUNNING_GLOW.gamma_multiply((0.3 + 0.5 * running_pulse) * fade);
            painter.circle_stroke(center, r + step * (k as f32 + 0.5), Stroke::new(step * 1.2, glow));
        }
    }

    let light = egui::emath::Vec2::new(-0.45, -0.55);
    let light_z = (1.0 - light.length_sq()).sqrt();
    let (texture, uv_half, base) = match look.texture {
        Some((id, size)) => (id, vec2((size.y / size.x).min(1.0), (size.x / size.y).min(1.0)) * 0.5, Color32::WHITE),
        None => (TextureId::default(), Vec2::ZERO, look.fill),
    };
    if !look.empty {
        painter.add(disc_mesh(texture, 10, 56, |rho, theta| {
        let dir = Vec2::angled(theta);
        let z = (1.0 - rho * rho).max(0.0).sqrt();
        let lit = 0.62 + 0.38 * (dir.dot(light) * rho + z * light_z).max(0.0);
        let rim = 1.0 - 0.3 * rho.powi(4);
        let uv = if look.texture.is_some() { pos2(0.5, 0.5) + dir * (rho.asin() / FRAC_PI_2) * uv_half * 2.0 } else { white_uv };
        (center + dir * rho * r, uv, scale_color(base, lit * rim))
        }));
    }
    if !look.empty && look.texture.is_none() && !look.label.is_empty() {
        shadow_text(painter, center, Align2::CENTER_CENTER, &look.label, r * 0.5, Color32::WHITE);
    }

    painter.add(disc_mesh(TextureId::default(), 8, 56, |rho, theta| {
        let hue = (theta / TAU + t * 0.04 + rho * 0.35).rem_euclid(1.0);
        let strength = 0.03 + 0.5 * rho.powi(3);
        let rgb = egui::ecolor::Hsva::new(hue, 0.6, 1.0, 1.0).to_rgb();
        let c = |v: f32| (v * strength * 255.0) as u8;
        let color = Color32::from_rgba_premultiplied(c(rgb[0]), c(rgb[1]), c(rgb[2]), (strength * 90.0) as u8);
        (center + Vec2::angled(theta) * rho * r, white_uv, color)
    }));

    let highlight = |at: Vec2, radii: Vec2, angle: f32, alpha: f32| {
        let turn = egui::emath::Rot2::from_angle(angle);
        disc_mesh(TextureId::default(), 6, 32, move |rho, theta| {
            let fade = (1.0 - rho * rho).powi(2);
            let pos = center + at * r + turn * (Vec2::angled(theta) * rho * radii * r);
            (pos, white_uv, Color32::from_white_alpha((alpha * fade) as u8))
        })
    };
    painter.add(highlight(vec2(-0.36, -0.42), vec2(0.34, 0.19), -0.65, 200.0));
    painter.add(highlight(vec2(0.44, 0.47), vec2(0.11, 0.07), -0.65, 90.0));

    painter.circle_stroke(center, r, Stroke::new(1.4 * s, Color32::from_white_alpha(90)));
    painter.circle_stroke(center, r * 0.96, Stroke::new(1.0 * s, Color32::from_white_alpha(22)));
    if look.swapping {
        let pulse = 0.75 + 0.25 * (t * 4.0).sin();
        for k in 0..8 {
            let alpha = pulse * 0.5 * (1.0 - k as f32 / 8.0).powi(2);
            painter.circle_stroke(center, r + (2.0 + k as f32 * 3.0) * s, Stroke::new(4.0 * s, SWAP_GLOW.gamma_multiply(alpha)));
        }
        painter.circle_stroke(center, r, Stroke::new(3.0 * s, SWAP_GLOW.gamma_multiply(pulse)));
    }
    if look.running {
        painter.circle_stroke(center, r, Stroke::new((3.0 + 2.0 * running_pulse) * s, RUNNING_GLOW.gamma_multiply(0.6 + 0.4 * running_pulse)));
    }
    if focus > 0.0 {
        painter.circle_stroke(center, r + 5.0 * s, Stroke::new(3.0 * s, Color32::WHITE.gamma_multiply(focus)));
        painter.circle_stroke(center, r + 10.0 * s, Stroke::new(4.0 * s, Color32::from_white_alpha((40.0 * focus) as u8)));
    }
}

fn cycle<T: Copy + PartialEq>(all: &[T], current: T, delta: i32) -> T {
    let i = all.iter().position(|x| *x == current).unwrap_or(0) as i32;
    all[(i + delta).rem_euclid(all.len() as i32) as usize]
}

fn cycle_or_default<T: Copy + PartialEq>(all: &[T], current: Option<T>, delta: i32) -> Option<T> {
    let i = current.and_then(|c| all.iter().position(|x| *x == c)).map_or(0, |i| i as i32 + 1);
    let next = (i + delta).rem_euclid(all.len() as i32 + 1);
    (next > 0).then(|| all[next as usize - 1])
}

fn add_menu_index(source: AddSource) -> usize {
    ADD_MENU.iter().position(|(s, ..)| *s == source).unwrap_or(0)
}

fn paint_checkbox(painter: &Painter, rect: Rect, checked: bool, s: f32) {
    let radius = (4.0 * s) as u8;
    if checked {
        painter.rect_filled(rect, radius, ACCENT);
        painter.text(rect.center(), Align2::CENTER_CENTER, "✔", FontId::proportional(rect.height() * 0.75), Color32::WHITE);
    } else {
        painter.rect_stroke(rect, radius, Stroke::new(2.0 * s, Color32::from_gray(160)), StrokeKind::Inside);
    }
}

fn side_panel(ui: &Ui, screen: Rect, s: f32, title: &str, rows: &[(&str, Option<&str>)], cursor: &mut usize) -> Option<usize> {
    let panel = Rect::from_min_max(pos2(screen.right() - 520.0 * s, screen.top()), screen.max);
    let painter = ui.painter();
    painter.rect_filled(screen, 0, Color32::from_black_alpha(80));
    painter.rect_filled(panel, 0, Color32::from_rgba_unmultiplied(10, 16, 32, 240));
    painter.text(pos2(panel.left() + 40.0 * s, panel.top() + 110.0 * s), Align2::LEFT_TOP, title, FontId::proportional(30.0 * s), Color32::WHITE);

    let row_h = if rows.iter().any(|(_, sub)| sub.is_some()) { 84.0 } else { 60.0 } * s;
    let mut clicked = None;
    for (i, (label, sub)) in rows.iter().enumerate() {
        let row = Rect::from_min_size(pos2(panel.left(), panel.top() + 190.0 * s + i as f32 * (row_h + 6.0 * s)), vec2(panel.width(), row_h));
        let resp = ui.interact(row, Id::new(("panel-row", i)), Sense::click());
        if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
            *cursor = i;
        }
        if resp.clicked() {
            clicked = Some(i);
        }
        let active = i == *cursor;
        if active {
            painter.rect_filled(row, 0, Color32::from_white_alpha(22));
            painter.rect_filled(Rect::from_min_size(row.min, vec2(5.0 * s, row.height())), 0, ACCENT);
        }
        let color = if active { Color32::WHITE } else { Color32::from_gray(180) };
        let x = row.left() + 40.0 * s;
        match sub {
            Some(sub) => {
                painter.text(pos2(x, row.center().y - 4.0 * s), Align2::LEFT_BOTTOM, *label, FontId::proportional(24.0 * s), color);
                painter.text(pos2(x, row.center().y + 4.0 * s), Align2::LEFT_TOP, *sub, FontId::proportional(16.0 * s), Color32::from_gray(140));
            }
            None => {
                painter.text(pos2(x, row.center().y), Align2::LEFT_CENTER, *label, FontId::proportional(24.0 * s), color);
            }
        }
    }
    clicked
}

fn draw_game_tile(ui: &Ui, game: &Game, rect: Rect, radius: u8) {
    if let Some(cover) = &game.cover {
        let placement = game.cover_placement;
        if placement.zoom < 1.0 {
            ui.painter().rect_filled(rect, radius, placeholder_color(&game.name).gamma_multiply(0.5));
        }
        paint_image_placed(ui, cover, rect, 1.0, radius, placement);
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, radius, placeholder_color(&game.name));
    painter.text(rect.center(), Align2::CENTER_CENTER, initials(&game.name), FontId::proportional(rect.height() * 0.3), Color32::WHITE);
}

fn object_floor(rect: Rect) -> (Pos2, Vec2) {
    (pos2(rect.center().x, rect.bottom() - rect.height() * 0.08), vec2(rect.width() * 0.36, rect.height() * 0.06))
}

fn placeholder_color(name: &str) -> Color32 {
    let hue = name.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) % 360;
    egui::ecolor::Hsva::new(hue as f32 / 360.0, 0.5, 0.45, 1.0).into()
}

fn initials(name: &str) -> String {
    let initials: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(3).collect();
    initials.to_uppercase()
}

struct BoxShape {
    front: Rect,
    depth: Vec2,
}

impl BoxShape {
    const RECEDE: Vec2 = vec2(-0.8, -0.6);

    fn fit(rect: Rect, [w, h, d]: [f32; 3]) -> Self {
        let (aspect, depth_ratio) = (w / h, d / h);
        let extra = -Self::RECEDE * depth_ratio;
        let h = (rect.height() / (1.0 + extra.y)).min(rect.width() / (aspect + extra.x));
        let depth = Self::RECEDE * depth_ratio * h;
        let size = vec2(h * aspect, h);
        let corner = rect.center() - (size - depth) / 2.0;
        BoxShape { front: Rect::from_min_size(corner - depth, size), depth }
    }

    fn spine(&self) -> [Pos2; 4] {
        let f = self.front;
        [f.left_top() + self.depth, f.left_top(), f.left_bottom(), f.left_bottom() + self.depth]
    }

    fn top(&self) -> [Pos2; 4] {
        let f = self.front;
        [f.left_top() + self.depth, f.right_top() + self.depth, f.right_top(), f.left_top()]
    }

    fn outline(&self) -> [Pos2; 6] {
        let (f, d) = (self.front, self.depth);
        [f.left_bottom(), f.left_bottom() + d, f.left_top() + d, f.right_top() + d, f.right_top(), f.right_bottom()]
    }
}

fn paint_image_quad(ui: &Ui, path: &Path, quad: [Pos2; 4], tint: Color32, placement: CoverPlacement) -> bool {
    let size = vec2(quad[0].distance(quad[1]), quad[0].distance(quad[3]));
    let texture = match egui::Image::new(file_uri(path)).load_for_size(ui.ctx(), size) {
        Ok(egui::load::TexturePoll::Ready { texture }) => texture,
        _ => return false,
    };
    let turn = box3d::turns_to_fit(texture.size, size);
    let (part, uv) = box3d::place_turned(texture.size, size, placement, turn);
    if !part.is_positive() {
        return true;
    }
    let at = |p: Pos2| quad[0].lerp(quad[1], p.x).lerp(quad[3].lerp(quad[2], p.x), p.y);
    let corners = [part.left_top(), part.right_top(), part.right_bottom(), part.left_bottom()].map(at);
    let mut mesh = Mesh::with_texture(texture.id);
    for (pos, uv) in corners.into_iter().zip(uv) {
        mesh.vertices.push(egui::epaint::Vertex { pos, uv, color: tint });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
    true
}

fn paint_spine_text(ui: &Ui, text: &str, font: egui::FontFamily, color: Color32, b: &BoxShape) {
    let painter = ui.painter();
    let width = b.depth.length();
    let max_len = b.front.height() * 0.88;
    let mut size = width * 0.5;
    let mut galley = painter.layout_no_wrap(text.to_owned(), FontId::new(size, font.clone()), color);
    if galley.size().x > max_len {
        size *= max_len / galley.size().x;
        galley = painter.layout_no_wrap(text.to_owned(), FontId::new(size.max(1.0), font), color);
    }
    let center = b.front.left_center() + b.depth / 2.0;
    let (down, back) = (vec2(0.0, 1.0), b.depth / width);
    let half = galley.size() / 2.0;
    let [w, h] = ui.ctx().fonts(|f| f.font_image_size());
    let uv_scale = vec2(1.0 / w as f32, 1.0 / h as f32);
    let mut mesh = Mesh::default();
    for row in &galley.rows {
        let first = mesh.vertices.len() as u32;
        mesh.indices.extend(row.visuals.mesh.indices.iter().map(|i| i + first));
        mesh.vertices.extend(row.visuals.mesh.vertices.iter().map(|v| {
            let p = row.pos.to_vec2() + v.pos.to_vec2() - half;
            egui::epaint::Vertex {
                pos: center + down * p.x + back * p.y,
                uv: (v.uv.to_vec2() * uv_scale).to_pos2(),
                color: if v.color == Color32::PLACEHOLDER { color } else { v.color },
            }
        }));
    }
    painter.add(Shape::mesh(mesh));
}

fn paint_image_cover(ui: &Ui, path: &Path, rect: Rect, alpha: f32, radius: u8) {
    paint_image_placed(ui, path, rect, alpha, radius, CoverPlacement::default());
}

fn paint_image_placed(ui: &Ui, path: &Path, rect: Rect, alpha: f32, radius: u8, placement: CoverPlacement) {
    paint_placed(ui, egui::Image::new(file_uri(path)), rect, alpha, radius, placement);
}

fn paint_placed(ui: &Ui, image: egui::Image, rect: Rect, alpha: f32, radius: u8, placement: CoverPlacement) {
    let image = image.show_loading_spinner(false).tint(Color32::WHITE.gamma_multiply(alpha)).corner_radius(radius);
    let Some(size) = image.load_for_size(ui.ctx(), rect.size()).ok().and_then(|t| t.size()) else {
        return image.paint_at(ui, rect);
    };
    let (part, uv) = box3d::place(size, rect.size(), placement);
    if part.is_positive() {
        let target = Rect::from_min_max(rect.lerp_inside(part.min.to_vec2()), rect.lerp_inside(part.max.to_vec2()));
        image.uv(uv).paint_at(ui, target);
    }
}

pub(crate) fn file_uri(path: &Path) -> String {
    let p = path.to_string_lossy();
    if p.starts_with('/') { format!("file://{p}") } else { format!("file:///{p}") }
}

fn section_of(sections: &[SettingSection], row: usize) -> usize {
    sections.iter().position(|sec| sec.rows.contains(&row)).unwrap_or(0)
}

fn follow_scroll(ui: &Ui, id: Id, top: f32, h: f32, content_h: f32, view_h: f32) -> f32 {
    let mut scroll: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    scroll = scroll.min(top).max(top + h - view_h);
    scroll = scroll.clamp(0.0, (content_h - view_h).max(0.0));
    ui.data_mut(|d| d.insert_temp(id, scroll));
    ui.ctx().animate_value_with_time(id, scroll, 0.12)
}

fn wheel_lines(ui: &Ui) -> f32 {
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

fn value_arrows(painter: &Painter, row: Rect, right: f32, inner: f32, active: bool, s: f32) -> [Rect; 2] {
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

fn gradient(painter: &Painter, rect: Rect, from: Color32, to: Color32, direction: GradientDirection) {
    let mid = from.lerp_to_gamma(to, 0.5);
    let corners = match direction {
        GradientDirection::Vertical => [from, from, to, to],
        GradientDirection::Horizontal => [from, to, from, to],
        GradientDirection::Diagonal => [from, mid, mid, to],
    };
    let mut mesh = egui::Mesh::default();
    for (pos, color) in [rect.left_top(), rect.right_top(), rect.left_bottom(), rect.right_bottom()].into_iter().zip(corners) {
        mesh.colored_vertex(pos, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(egui::Shape::mesh(mesh));
}

fn paint_theme_background(ui: &Ui, theme: &Theme, rect: Rect) {
    let bg = &theme.settings.background;
    let painter = ui.painter();
    match bg.kind {
        BackgroundKind::Color => {
            painter.rect_filled(rect, 0, bg.color);
        }
        BackgroundKind::Gradient => gradient(painter, rect, bg.gradient_from, bg.gradient_to, bg.gradient_direction),
        BackgroundKind::Image => {
            painter.rect_filled(rect, 0, bg.color);
            if let Some(path) = theme.background_image() {
                paint_image_fit(ui, &path, rect, bg.image_fit);
            }
        }
    }
}

fn paint_widget_item(painter: &Painter, item: &addons::Item, cell: Rect, s: f32) {
    let x = cell.center().x;
    let mid = cell.center().y;
    let meter = item.level.map(|l| l.clamp(0.0, 1.0));
    let shift = if meter.is_some() { -4.0 * s } else { 0.0 };
    shadow_text(painter, pos2(x, mid - 18.0 * s + shift), Align2::CENTER_CENTER, &item.label, 15.0 * s, Color32::from_gray(170));
    shadow_text(painter, pos2(x, mid + 6.0 * s + shift), Align2::CENTER_CENTER, &item.value, 24.0 * s, Color32::WHITE);
    if let Some(level) = meter {
        let track = Rect::from_center_size(pos2(x, mid + 28.0 * s), vec2(cell.width() - 30.0 * s, 6.0 * s));
        let color = item.color().unwrap_or(match level {
            ..0.6 => PAD_A,
            ..0.85 => PAD_Y,
            _ => PAD_B,
        });
        painter.rect_filled(track, (3.0 * s) as u8, Color32::from_white_alpha(30));
        painter.rect_filled(Rect::from_min_size(track.min, vec2(track.width() * level, track.height())), (3.0 * s) as u8, color);
    }
}

fn paint_bar(ui: &Ui, theme: &Theme, slot: BarSlot, rect: Rect) {
    let bar = theme.settings.bar(slot);
    if !bar.enabled {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    match bar.kind {
        BackgroundKind::Color => {
            painter.rect_filled(rect, 0, bar.color);
        }
        BackgroundKind::Gradient => gradient(&painter, rect, bar.gradient_from, bar.gradient_to, bar.gradient_direction),
        BackgroundKind::Image => {
            painter.rect_filled(rect, 0, bar.color);
            if let Some(path) = theme.bar_image(slot) {
                paint_bar_image(ui, &painter, &path, rect, bar);
            }
        }
    }
}

fn paint_bar_image(ui: &Ui, painter: &Painter, path: &Path, rect: Rect, bar: &BarStyle) {
    let Ok(egui::load::TexturePoll::Ready { texture }) = egui::Image::new(file_uri(path)).load_for_size(ui.ctx(), rect.size()) else {
        return;
    };
    if texture.size.x <= 0.0 || texture.size.y <= 0.0 {
        return;
    }
    let cover = (rect.width() / texture.size.x).max(rect.height() / texture.size.y);
    let half = texture.size * cover * bar.image_zoom / 2.0;
    let center = rect.center() + vec2(bar.image_x * rect.width(), bar.image_y * rect.height());
    let turn = egui::emath::Rot2::from_angle(bar.image_rotation.to_radians());
    let mut mesh = Mesh::with_texture(texture.id);
    for (corner, uv) in [(vec2(-1.0, -1.0), pos2(0.0, 0.0)), (vec2(1.0, -1.0), pos2(1.0, 0.0)), (vec2(1.0, 1.0), pos2(1.0, 1.0)), (vec2(-1.0, 1.0), pos2(0.0, 1.0))] {
        mesh.vertices.push(egui::epaint::Vertex { pos: center + turn * (corner * half), uv, color: Color32::WHITE });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

fn paint_image_fit(ui: &Ui, path: &Path, rect: Rect, fit: ImageFit) {
    if fit == ImageFit::Fill {
        return paint_image_cover(ui, path, rect, 1.0, 0);
    }
    let options = match fit {
        ImageFit::Tile => TextureOptions::LINEAR_REPEAT,
        ImageFit::Mirror => TextureOptions::LINEAR_MIRRORED_REPEAT,
        _ => TextureOptions::LINEAR,
    };
    let image = egui::Image::new(file_uri(path)).show_loading_spinner(false).texture_options(options);
    let Some(texels) = image.load_for_size(ui.ctx(), rect.size()).ok().and_then(|t| t.size()) else { return };
    let natural = texels / ui.ctx().pixels_per_point();
    let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let (target, uv) = match fit {
        ImageFit::Stretch => (rect, full),
        ImageFit::Fit => {
            let scale = (rect.width() / texels.x).min(rect.height() / texels.y);
            (Rect::from_center_size(rect.center(), texels * scale), full)
        }
        ImageFit::Center => (Rect::from_center_size(rect.center(), natural), full),
        ImageFit::Tile | ImageFit::Mirror => {
            (rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(rect.width() / natural.x, rect.height() / natural.y)))
        }
        ImageFit::Fill => unreachable!(),
    };
    image.uv(uv).paint_at(ui, target);
}

struct ButtonSkin {
    fill: Color32,
    text: Color32,
    icon: Option<PathBuf>,
}

impl ButtonSkin {
    fn plain(fill: Color32) -> Self {
        ButtonSkin { fill, text: Color32::WHITE, icon: None }
    }
}

fn button_icon(painter: &Painter, skin: &ButtonSkin, h: f32) -> Option<(egui::TextureId, Vec2)> {
    let image = egui::Image::new(file_uri(skin.icon.as_deref()?));
    let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(painter.ctx(), vec2(h, h)) else { return None };
    let aspect = (texture.size.x / texture.size.y.max(1.0)).clamp(0.25, 4.0);
    Some((texture.id, vec2(h * aspect, h)))
}

fn button_width(painter: &Painter, glyph: &str, skin: &ButtonSkin, font: &FontId, h: f32, s: f32) -> f32 {
    if let Some((_, size)) = button_icon(painter, skin, h) {
        return size.x;
    }
    (painter.layout_no_wrap(glyph.to_owned(), font.clone(), skin.text).size().x + 16.0 * s).max(h)
}

fn paint_button(painter: &Painter, left_center: Pos2, glyph: &str, skin: &ButtonSkin, font: FontId, h: f32, s: f32) -> Rect {
    if let Some((texture, size)) = button_icon(painter, skin, h) {
        let rect = Rect::from_min_size(pos2(left_center.x, left_center.y - h / 2.0), size);
        painter.image(texture, rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        return rect;
    }
    let galley = painter.layout_no_wrap(glyph.to_owned(), font, skin.text);
    let w = (galley.size().x + 16.0 * s).max(h);
    let pill = Rect::from_min_size(pos2(left_center.x, left_center.y - h / 2.0), vec2(w, h));
    painter.rect_filled(pill, (h / 2.0) as u8, skin.fill);
    painter.galley(pill.center() - galley.size() / 2.0, galley, skin.text);
    pill
}

fn glyph_color(glyph: &str) -> Color32 {
    match glyph {
        "A" => PAD_A,
        "B" => PAD_B,
        "X" => PAD_X,
        "Y" => PAD_Y,
        _ => PAD_MENU,
    }
}

fn keyboard_key(action: Action) -> &'static str {
    match action {
        Action::Confirm => "Enter",
        Action::Back => "Esc",
        Action::Options => "Tab",
        Action::Add => "Ins",
        Action::Erase => "Bksp",
        Action::ToggleFullscreen => "F11",
        _ => "",
    }
}

fn button_token(button: PadButton) -> String {
    format!("{BUTTON_OPEN}{}{BUTTON_CLOSE}", button.glyph())
}

fn face_token(button: FaceButton) -> String {
    format!("{BUTTON_OPEN}{}{BUTTON_CLOSE}", button.glyph())
}

#[allow(clippy::too_many_arguments)]
fn text_with_buttons(painter: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32, s: f32, skin: &dyn Fn(&str) -> ButtonSkin) {
    if !text.contains(BUTTON_OPEN) {
        painter.text(pos, align, text, font, color);
        return;
    }
    let h = font.size * 1.35;
    let y = match align.y() {
        egui::Align::Min => pos.y + h / 2.0,
        egui::Align::Center => pos.y,
        egui::Align::Max => pos.y - h / 2.0,
    };
    let mut x = pos.x;
    for (i, part) in text.split(BUTTON_OPEN).enumerate() {
        let (glyph, rest) = match part.split_once(BUTTON_CLOSE) {
            Some((glyph, rest)) if i > 0 => (Some(glyph), rest),
            _ => (None, part),
        };
        if let Some(glyph) = glyph {
            x = paint_button(painter, pos2(x, y), glyph, &skin(glyph), font.clone(), h, s).right();
        }
        if !rest.is_empty() {
            x = painter.text(pos2(x, y), Align2::LEFT_CENTER, rest, font.clone(), color).right();
        }
    }
}

fn shadow_text(painter: &Painter, pos: egui::Pos2, align: Align2, text: &str, size: f32, color: Color32) {
    shadow_text_font(painter, pos, align, text, FontId::proportional(size), color);
}

fn shadow_text_font(painter: &Painter, pos: egui::Pos2, align: Align2, text: &str, font: FontId, color: Color32) {
    painter.text(pos + vec2(0.0, 2.0), align, text, font.clone(), Color32::from_black_alpha(160));
    painter.text(pos, align, text, font, color);
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
