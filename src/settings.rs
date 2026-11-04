
use crate::input::{Bindings, Control, PadButton, PadFamily};
use crate::library::Library;
use crate::stores::Store;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const FILE: &str = "userSettings.json";
const LEGACY_FILE: &str = "settings.json";

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct StoreSettings {
    pub enabled: bool,
    pub start_in_background: bool,
    pub command: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum DiscAction {
    #[default]
    Start,
    Ask,
    Ignore,
}

impl DiscAction {
    pub const ALL: [DiscAction; 3] = [DiscAction::Start, DiscAction::Ask, DiscAction::Ignore];

    pub fn label(self) -> &'static str {
        match self {
            DiscAction::Start => tr!("Start the game"),
            DiscAction::Ask => tr!("Ask first"),
            DiscAction::Ignore => tr!("Do nothing"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct DiscSettings {
    pub on_insert: DiscAction,
    pub scan_at_start: bool,
    pub removable_drives: bool,
    /// Copy a game's saves to the USB stick or SD card it was started from when it closes, for
    /// the games that have it turned on.
    pub keep_saves: bool,
}

impl Default for DiscSettings {
    fn default() -> Self {
        DiscSettings { on_insert: DiscAction::Start, scan_at_start: true, removable_drives: true, keep_saves: false }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct MouseSettings {
    pub virtual_mouse: bool,
    pub speed: f32,
    pub cursor: String,
    pub cursor_size: f32,
}

impl MouseSettings {
    pub const SPEED_RANGE: (f32, f32) = (300.0, 3000.0);
    pub const SIZE_RANGE: (f32, f32) = (16.0, 128.0);

    pub fn cursor_path(&self) -> Option<PathBuf> {
        (!self.cursor.is_empty()).then(|| Library::data_dir().join(&self.cursor))
    }
}

impl Default for MouseSettings {
    fn default() -> Self {
        MouseSettings { virtual_mouse: false, speed: 1200.0, cursor: String::new(), cursor_size: 32.0 }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct ControlSettings {
    pub remap: BTreeMap<PadButton, Control>,
    pub swap_ab: bool,
    /// Which controller's buttons the hints show; `None` follows the connected controller.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PadFamily>,
}

impl ControlSettings {
    pub fn is_custom(&self) -> bool {
        !self.remap.is_empty()
    }

    pub fn bindings(&self) -> Bindings {
        let mut bindings = Bindings::default();
        if self.is_custom() {
            for (&button, &control) in &self.remap {
                bindings.set(button, control);
            }
        } else if self.swap_ab {
            bindings.set(PadButton::South, Control::Back);
            bindings.set(PadButton::East, Control::Confirm);
        }
        bindings
    }

    pub fn bind(&mut self, button: PadButton, control: Control) {
        let mut bindings = self.bindings();
        bindings.set(button, control);
        self.remap = PadButton::ALL
            .into_iter()
            .filter(|b| bindings.get(*b) != b.default_control())
            .map(|b| (b, bindings.get(b)))
            .collect();
        self.swap_ab = false;
    }

    pub fn reset(&mut self) {
        self.remap.clear();
        self.swap_ab = false;
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct FontSettings {
    pub title: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    English,
    French,
    Spanish,
    Polish,
}

impl Language {
    pub const ALL: [Self; 4] = [Self::English, Self::French, Self::Spanish, Self::Polish];

    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::French => "Français",
            Self::Spanish => "Español",
            Self::Polish => "Polski",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct AppStoreSettings {
    pub url: String,
    pub api_key: String,
    pub name: String,
}

static NO_APP_STORE: AppStoreSettings = AppStoreSettings { url: String::new(), api_key: String::new(), name: String::new() };

impl AppStoreSettings {
    pub fn is_empty(&self) -> bool {
        self.url.trim().is_empty() && self.api_key.trim().is_empty()
    }

    pub fn title(&self) -> String {
        let url = self.url.trim();
        let host = url.split("://").last().unwrap_or(url).split('/').next().unwrap_or_default();
        match (self.name.trim(), host) {
            ("", "") => tr!("Not set up").to_owned(),
            ("", host) => host.to_owned(),
            (name, _) => name.to_owned(),
        }
    }

    /// The store to talk to; only the address is needed. A key saved by older versions is still sent.
    pub fn endpoint(&self) -> Option<crate::appstore::Endpoint> {
        let base = crate::appstore::api_base(&self.url)?;
        Some(crate::appstore::Endpoint { base, key: self.api_key.trim().to_owned() })
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AddonSettings {
    pub bar: Option<crate::theme::BarSlot>,
    /// Drawn anywhere on the screen at (`x`, `y`) instead of in `bar`.
    pub on_screen: bool,
    /// The widget's center, as a fraction of the screen's width and height.
    pub x: f32,
    pub y: f32,
    /// Percent of the widget's normal size.
    pub size: u32,
}

impl AddonSettings {
    pub const SIZE_RANGE: (u32, u32) = (50, 300);

    pub fn scale(&self) -> f32 {
        self.size.clamp(Self::SIZE_RANGE.0, Self::SIZE_RANGE.1) as f32 / 100.0
    }
}

impl Default for AddonSettings {
    fn default() -> Self {
        AddonSettings { bar: None, on_screen: false, x: 0.5, y: 0.5, size: 100 }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct ShortcutSettings {
    pub store: Option<crate::theme::BarSlot>,
    pub add_game: Option<crate::theme::BarSlot>,
}

/// How the row and grid views order the library.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct OrderSettings {
    /// Starting a game moves it to the first slot no game is fixed to.
    pub last_played_first: bool,
    /// Games only move when swapped by hand: nothing reorders them on its own.
    #[serde(rename = "static")]
    pub static_order: bool,
}

impl OrderSettings {
    pub fn moves_played(&self) -> bool {
        self.last_played_first && !self.static_order
    }
}

/// What happens to a game's images when new ones replace them.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct GallerySettings {
    /// Replaced images stay in the game's gallery until removed by hand.
    pub keep_all: bool,
}

impl Default for GallerySettings {
    fn default() -> Self {
        GallerySettings { keep_all: true }
    }
}

impl Default for OrderSettings {
    fn default() -> Self {
        OrderSettings { last_played_first: true, static_order: false }
    }
}

/// Windows-only system behavior.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct WindowsSettings {
    /// Hides the taskbar on every monitor while YASAFF is fullscreen, so it can't show over it.
    pub hide_taskbar: bool,
}

impl Default for WindowsSettings {
    fn default() -> Self {
        WindowsSettings { hide_taskbar: true }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Settings {
    pub start_fullscreen: bool,
    pub language: Language,
    pub theme: String,
    pub heroic: StoreSettings,
    pub steam: StoreSettings,
    pub discs: DiscSettings,
    pub mouse: MouseSettings,
    pub controls: ControlSettings,
    pub shortcuts: ShortcutSettings,
    pub fonts: FontSettings,
    pub order: OrderSettings,
    pub gallery: GallerySettings,
    pub windows: WindowsSettings,
    pub app_stores: Vec<AppStoreSettings>,
    pub active_app_store: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_dir: Option<PathBuf>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub addons: BTreeMap<String, AddonSettings>,
    #[serde(skip_serializing)]
    app_store: Option<AppStoreSettings>,
    /// The settings file could be neither read nor moved aside; saving would destroy it.
    #[serde(skip)]
    keep_file: bool,
}

impl Settings {
    fn file() -> PathBuf {
        Library::data_dir().join(FILE)
    }

    /// Reads the settings, falling back to defaults. A file that can't be read or parsed is moved
    /// aside (rather than overwritten by the next save) and described in the returned warning;
    /// one that can't be moved either is kept, and saving is refused for this session.
    pub fn load() -> (Self, Option<String>) {
        let mut file = Self::file();
        if !file.exists() {
            file = Library::data_dir().join(LEGACY_FILE);
        }
        if !file.exists() {
            return (Self::default(), None);
        }
        let read = fs::read_to_string(&file).map_err(anyhow::Error::from).and_then(|text| Ok(serde_json::from_str::<Self>(&text)?));
        match read {
            Ok(settings) => (settings.migrated(), None),
            Err(e) => {
                let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let backup = file.with_extension(format!("json.broken-{stamp}"));
                log::warn!("ignoring unreadable {}: {e:#}", file.display());
                match fs::rename(&file, &backup) {
                    Ok(()) => (Self::default(), Some(format!("{}: {e:#} (moved to {})", file.display(), backup.display()))),
                    Err(_) => (
                        Self { keep_file: true, ..Self::default() },
                        Some(format!("{}: {e:#} (left as it is; changes won't be saved)", file.display())),
                    ),
                }
            }
        }
    }

    fn migrated(mut self) -> Self {
        if let Some(old) = self.app_store.take()
            && !old.is_empty()
            && self.app_stores.is_empty()
        {
            self.app_stores.push(old);
        }
        self
    }

    pub fn app_store(&self) -> &AppStoreSettings {
        self.app_stores.get(self.active_app_store).or(self.app_stores.last()).unwrap_or(&NO_APP_STORE)
    }

    pub fn app_store_mut(&mut self) -> &mut AppStoreSettings {
        if self.app_stores.is_empty() {
            self.app_stores.push(AppStoreSettings::default());
        }
        self.active_app_store = self.active_app_store.min(self.app_stores.len() - 1);
        &mut self.app_stores[self.active_app_store]
    }

    pub fn add_app_store(&mut self) {
        if self.app_stores.is_empty() || !self.app_store().is_empty() {
            self.app_stores.push(AppStoreSettings::default());
            self.active_app_store = self.app_stores.len() - 1;
        }
    }

    pub fn remove_app_store(&mut self) {
        if self.active_app_store < self.app_stores.len() {
            self.app_stores.remove(self.active_app_store);
        }
        self.active_app_store = self.active_app_store.saturating_sub(1);
    }

    pub fn save(&self) -> Result<()> {
        anyhow::ensure!(!self.keep_file, "{} could not be read at startup and is left as it is", Self::file().display());
        crate::fsutil::write_atomic(&Self::file(), serde_json::to_string_pretty(self)?)?;
        let _ = fs::remove_file(Library::data_dir().join(LEGACY_FILE));
        Ok(())
    }

    pub fn import_cursor(&mut self, image: &std::path::Path) -> Result<()> {
        let ext = image.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
        let name = format!("cursor-{}.{ext}", uuid::Uuid::new_v4());
        let dir = Library::data_dir();
        fs::create_dir_all(&dir)?;
        fs::copy(image, dir.join(&name))?;
        self.clear_cursor();
        self.mouse.cursor = name;
        self.save()
    }

    pub fn clear_cursor(&mut self) {
        if let Some(old) = self.mouse.cursor_path() {
            let _ = fs::remove_file(old);
        }
        self.mouse.cursor.clear();
    }

    pub fn store(&self, store: Store) -> &StoreSettings {
        match store {
            Store::Heroic => &self.heroic,
            Store::Steam => &self.steam,
        }
    }

    pub fn store_mut(&mut self, store: Store) -> &mut StoreSettings {
        match store {
            Store::Heroic => &mut self.heroic,
            Store::Steam => &mut self.steam,
        }
    }

    pub fn store_command(&self, store: Store) -> Option<Vec<String>> {
        let custom = self.store(store).command.trim();
        if custom.is_empty() {
            store.detect()
        } else {
            Some(crate::apps::split_command(custom)).filter(|c| !c.is_empty())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_only_applies_without_remapping() {
        let mut c = ControlSettings { swap_ab: true, ..Default::default() };
        assert_eq!(c.bindings().get(PadButton::East), Control::Confirm);
        assert_eq!(c.bindings().get(PadButton::South), Control::Back);

        c.bind(PadButton::LeftBumper, Control::Options);
        assert!(c.is_custom() && !c.swap_ab);
        assert_eq!(c.bindings().get(PadButton::East), Control::Confirm);
        assert_eq!(c.bindings().get(PadButton::LeftBumper), Control::Options);

        let saved: ControlSettings = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(saved.bindings(), c.bindings());

        c.bind(PadButton::LeftBumper, Control::Nothing);
        c.bind(PadButton::East, Control::Back);
        c.bind(PadButton::South, Control::Confirm);
        assert!(!c.is_custom());
        assert_eq!(c.bindings(), Bindings::default());
    }
}
