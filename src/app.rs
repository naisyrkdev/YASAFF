mod active_game;
mod addon_manager;
mod add_games;
mod buttons;
mod cover_editor;
mod dialogs;
mod discs;
mod font_ui;
mod home;
mod keyboard;
mod menus;
mod paint;
mod profile_ui;
mod saves_ui;
mod settings_ui;
mod store_screen;
mod tiles;

use crate::addons::{self, AddonHost, Placement};
use addon_manager::{AddonManager, AddonView};
use crate::apps::{self, AppEntry};
use crate::autostart;
use crate::box3d;
use crate::case3d;
use crate::crt;
use crate::disc3d;
use crate::fonts;
use crate::i18n;
use crate::initial;
use crate::global_mouse::GlobalMouse;
use crate::input::{Action, Control, GLOBAL_MOUSE_BUTTONS, Input, PadButton, PadFamily};
use crate::launcher;
use crate::library::{AssetKind, CoverPlacement, Game, Library};
use crate::metadata::{self, Fetcher};
use crate::object3d::{self, Objects};
use crate::profiles;
use crate::scan;
use crate::sound::Sounds;
use crate::settings::{DiscAction, Language, MouseSettings, Settings};
use crate::stores::Store;
use crate::video::{self, Videos};
use crate::theme::{
    self, BackgroundKind, BarPosition, BarSlot, BarStyle, BoxCase, BubbleSettings, ButtonShape, ButtonStyle, DiscKind, DiscSettings, DiscSpin, GradientDirection, GridAlign, GridSettings,
    CrtSettings, ImageFit, SpineLabel, Theme, ThemeKind, TileSettings, TileStyle, ViewKind,
};
use egui::{
    Align2, Color32, Context, FontId, Id, Mesh, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TextureHandle, TextureOptions,
    TextureId, Ui, Vec2, ViewportCommand, pos2, vec2,
};
use std::cell::RefCell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use crate::physical::burn::{self, BurnJob, Burner};
use active_game::*;
use add_games::*;
use buttons::*;
use cover_editor::*;
use discs::*;
use home::*;
use menus::*;
use paint::*;
pub(crate) use paint::file_uri;
use profile_ui::*;
use settings_ui::*;
use tiles::*;
use crate::watch::GameWatch;
use dialogs::Pick;
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
            Special::Store => "",
            Special::Add => "+",
            Special::Settings => "",
        }
    }

    /// Names the special in a theme's `icons`.
    fn key(self) -> &'static str {
        match self {
            Special::Store => "discover",
            Special::Add => "add",
            Special::Settings => "settings",
        }
    }

    /// The built-in icon, drawn in place of the glyph when present.
    fn icon(self) -> Option<egui::ImageSource<'static>> {
        match self {
            Special::Store => Some(egui::include_image!("../assets/ui/squares-plus.svg")),
            Special::Settings => Some(egui::include_image!("../assets/ui/gears.svg")),
            Special::Add => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Special::Store => tr!("Discover"),
            Special::Add => tr!("Add game"),
            Special::Settings => tr!("Settings"),
        }
    }
}

enum Mode {
    Home,
    Welcome { cursor: usize },
    Options { cursor: usize },
    SwapBubble { from: usize, target: usize },
    /// Picking the game to swap with the one at `from`; `selected` is the slot being pointed at.
    SwapTile { from: usize },
    AddMenu { cursor: usize },
    Picker(Box<Picker>),
    Scan(Box<ScanState>),
    ConfirmRemove { what: Removal, yes: bool },
    Prompt { kind: PromptKind, buffer: String },
    Settings { cursor: usize, page: Page, in_rows: bool },
    Launching(Launching),
    /// `candidates` are game ids: the library may be re-sorted while the chooser is open.
    DiscChooser { name: String, candidates: Vec<String>, cursor: usize },
    DiscNotFound { name: String },
    CoverEditor(CoverEditor),
    FontManager { cursor: usize, armed: bool },
    AddonManager(AddonManager),
    Burn(BurnDialog),
    Store { cursor: usize, scroll: usize },
    StoreTheme { index: usize, image: usize },
    ProfileImported { name: String, missing: Vec<String>, switch: bool },
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
    bubble_layout: Option<BubbleLayout>,
    bubble_empty: Option<usize>,
    sounds: Sounds,
    videos: RefCell<Videos>,
    crt_visible: Vec<usize>,
    fullscreen: bool,
    autostart: bool,
    bg_current: Option<PathBuf>,
    bg_previous: Option<PathBuf>,
    bg_changed: Instant,
    focus_guard_until: Option<Instant>,
    /// The selected tile and when it was selected; its 3D object turns from that moment.
    spin: (usize, Instant),
    /// Lazily loaded 3D models and their rendered textures. A `RefCell` because it is a render
    /// cache filled from the `&self` painting code, which never changes app state.
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
    /// Set once a restart was requested: the window is closing and nothing may touch the profile.
    restarting: bool,
    dialog: Option<dialogs::PendingDialog>,
    /// The button the theme's button rows in Settings change.
    button_edit: PadButton,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, settings: Settings, startup_warnings: Vec<String>, fullscreen: bool, disc: Option<PathBuf>) -> Self {
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
            bubble_layout: None,
            bubble_empty: None,
            sounds: Sounds::default(),
            videos: RefCell::default(),
            crt_visible: Vec::new(),
            fullscreen,
            autostart: autostart::is_enabled(),
            bg_current: None,
            bg_previous: None,
            bg_changed: Instant::now(),
            focus_guard_until: None,
            spin: (0, Instant::now()),
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
            restarting: false,
            dialog: None,
            button_edit: PadButton::South,
        };
        app.apply_fonts();
        if let Some(w) = warning {
            app.notify(tr!("Could not read library: {error}", error = w));
        }
        for w in startup_warnings {
            app.notify(w);
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
        log::info!("{msg}");
        self.toast = Some((msg, Instant::now()));
    }

    fn save(&mut self) {
        if let Err(e) = self.library.save() {
            self.notify(tr!("Could not save library: {error}", error = format!("{e:#}")));
        }
    }

    /// Gives every game the cover placement kept for the current tile style.
    fn sync_cover_shapes(&mut self) {
        let style = self.theme().settings.tile_style;
        let mut changed = false;
        for game in &mut self.library.games {
            changed |= game.use_cover_shape(style);
        }
        if changed {
            self.save();
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

    fn theme(&self) -> &Theme {
        &self.themes[self.theme]
    }

    /// The icon a special tile shows: the theme's own, else the built-in one.
    fn special_icon(&self, special: Special) -> Option<egui::ImageSource<'static>> {
        match self.theme().tile_icon(special.key()) {
            Some(path) => Some(egui::ImageSource::Uri(file_uri(&path).into())),
            None => special.icon(),
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

    fn handle(&mut self, ctx: &Context, action: Action) {
        if self.restarting || (self.dialog_open() && !matches!(action, Action::ToggleFullscreen)) {
            return;
        }
        match action {
            Action::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
                return;
            }
            Action::BringToFront => return self.come_home(ctx),
            _ => {}
        }

        match &self.mode {
            Mode::Home => self.handle_home(action),
            Mode::Options { .. } => self.handle_options(action),
            Mode::SwapBubble { .. } => self.handle_swap_bubble(action),
            Mode::SwapTile { .. } => self.handle_swap_tile(action),
            Mode::Welcome { .. } => self.handle_welcome(action),
            Mode::AddMenu { .. } => self.handle_add_menu(action),
            Mode::Picker(_) => self.handle_picker(action),
            Mode::Scan(_) => self.handle_scan(ctx, action),
            Mode::ConfirmRemove { .. } => self.handle_confirm_remove(action),
            Mode::Prompt { .. } => self.handle_prompt(action),
            Mode::Settings { .. } => self.handle_settings(action),
            Mode::CoverEditor(_) => self.handle_cover_editor(action),
            Mode::Launching(_) => {
                if action == Action::Back {
                    self.mode = Mode::Home;
                }
            }
            Mode::DiscChooser { .. } => self.handle_disc_chooser(action),
            Mode::DiscNotFound { .. } => {
                if matches!(action, Action::Confirm | Action::Back) {
                    self.mode = Mode::Home;
                }
            }
            Mode::ProfileImported { .. } => self.handle_profile_imported(action),
            Mode::Burn(_) => self.handle_burn(action),
            Mode::Store { .. } => self.handle_store(action),
            Mode::StoreTheme { .. } => self.handle_store_theme(action),
            Mode::FontManager { .. } => self.handle_font_manager(action),
            Mode::AddonManager(_) => self.handle_addon_manager(action),
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if self.restarting {
            return;
        }
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
        if self.restarting {
            let screen = ui.max_rect();
            ui.painter().rect_filled(screen, 0, Color32::BLACK);
            ui.painter().text(screen.center(), Align2::CENTER_CENTER, tr!("Restarting…"), FontId::proportional(32.0), Color32::WHITE);
            return;
        }
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
        self.poll_dialog();
        if ctx.input(|i| i.focused) {
            self.last_focused = Instant::now();
        }
        if let Some(loaded) = self.fonts_pending.take() {
            self.fonts = loaded;
        }
        self.sync_cover_shapes();
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
        self.update_videos(&ctx);
        self.guard_focus(&ctx);
        if self.spin.0 != self.selected {
            self.spin = (self.selected, Instant::now());
        }

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
        self.draw_addon_manager(ui, screen, s);
        self.draw_disc_chooser(ui, screen, s);
        self.draw_disc_not_found(ui, screen, s);
        self.draw_profile_imported(ui, screen, s);
        self.draw_burn(ui, screen, s);
        self.draw_setup(&painter, screen, s);
        self.draw_dialog_wait(&painter, screen, s);
        self.draw_toast(&painter, screen, s);
        self.draw_launching(ui, screen, s);
        self.draw_prompt(&ctx);
        self.draw_cursor(&ctx, s);

        let focused = ctx.input(|i| i.focused);
        ctx.request_repaint_after(Duration::from_millis(if focused { 16 } else { 100 }));
    }
}
