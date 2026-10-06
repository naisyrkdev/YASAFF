
use super::*;
use crate::appstore::{self, AppProcess, Endpoint, Event, Manifest, Placement, RemoteApp, RemoteTheme};
use crate::theme::StoreOrigin;
use std::collections::HashMap;

const QUICK_EXIT: Duration = Duration::from_secs(5);

pub(super) struct StoreState {
    client: appstore::Client,
    connection: Connection,
    listing: Listing,
    store_name: String,
    apps: Vec<RemoteApp>,
    icons: HashMap<String, PathBuf>,
    installs: HashMap<String, Install>,
    installed: HashMap<String, Manifest>,
    tab: StoreTab,
    themes: Vec<RemoteTheme>,
    theme_listing: Listing,
    previews: HashMap<(String, usize), PathBuf>,
    galleries: std::collections::HashSet<String>,
    theme_installs: HashMap<String, Install>,
    addons: Vec<RemoteApp>,
    addon_listing: Listing,
}

#[derive(Clone, Copy, PartialEq)]
enum StoreTab {
    Apps,
    Themes,
    Addons,
}

impl StoreTab {
    const ALL: [Self; 3] = [Self::Apps, Self::Themes, Self::Addons];

    fn label(self) -> &'static str {
        match self {
            Self::Apps => tr!("Apps"),
            Self::Themes => tr!("Themes"),
            Self::Addons => tr!("Addons"),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ThemeState {
    NotInstalled,
    Outdated(usize, u32),
    Installed(usize),
}

enum Connection {
    Unknown,
    Checking,
    Ok { store: String, user: String },
    Failed(String),
}

#[derive(PartialEq)]
enum Listing {
    NotLoaded,
    Loading,
    Loaded,
    Failed(String),
}

enum Install {
    Downloading(u64, u64),
    Failed(String),
}

pub(super) struct RunningApp {
    game_id: String,
    name: String,
    process: AppProcess,
    started: Instant,
}

impl StoreState {
    pub(super) fn new(ctx: Context, library: &Library) -> Self {
        let installed = library
            .games
            .iter()
            .filter_map(|g| g.app_id.as_deref())
            .filter_map(|id| Some((id.to_owned(), appstore::installed(id)?)))
            .collect();
        StoreState {
            client: appstore::Client::new(ctx),
            connection: Connection::Unknown,
            listing: Listing::NotLoaded,
            store_name: String::new(),
            apps: Vec::new(),
            icons: HashMap::new(),
            installs: HashMap::new(),
            installed,
            tab: StoreTab::Apps,
            themes: Vec::new(),
            theme_listing: Listing::NotLoaded,
            previews: HashMap::new(),
            galleries: Default::default(),
            theme_installs: HashMap::new(),
            addons: Vec::new(),
            addon_listing: Listing::NotLoaded,
        }
    }

    fn count(&self) -> usize {
        match self.tab {
            StoreTab::Apps => self.apps.len(),
            StoreTab::Themes => self.themes.len(),
            StoreTab::Addons => self.addons.len(),
        }
    }

    fn listing(&self) -> &Listing {
        match self.tab {
            StoreTab::Apps => &self.listing,
            StoreTab::Themes => &self.theme_listing,
            StoreTab::Addons => &self.addon_listing,
        }
    }

    fn packages(&self) -> &[RemoteApp] {
        match self.tab {
            StoreTab::Addons => &self.addons,
            _ => &self.apps,
        }
    }

    fn package_name(&self, id: &str) -> String {
        self.apps.iter().chain(&self.addons).find(|a| a.id == id).map_or(id.to_owned(), |a| a.name.clone())
    }

    fn update_for(&self, app_id: &str) -> Option<&RemoteApp> {
        let installed = self.installed.get(app_id)?;
        self.apps.iter().find(|a| a.id == app_id && a.latest.is_update_for(installed))
    }
}

impl App {
    fn store_endpoint(&self) -> Option<Endpoint> {
        self.settings.app_store().endpoint()
    }

    pub(super) fn open_store(&mut self) {
        self.mode = Mode::Store { cursor: 0, scroll: 0 };
        self.load_store_tab();
    }

    fn load_store_tab(&mut self) {
        match self.store.tab {
            StoreTab::Apps if matches!(self.store.listing, Listing::NotLoaded | Listing::Failed(_)) => self.refresh_store(),
            StoreTab::Themes if matches!(self.store.theme_listing, Listing::NotLoaded | Listing::Failed(_)) => self.refresh_store_themes(),
            StoreTab::Addons if matches!(self.store.addon_listing, Listing::NotLoaded | Listing::Failed(_)) => self.refresh_store_addons(),
            _ => {}
        }
    }

    fn refresh_store_addons(&mut self) {
        let Some(endpoint) = self.store_endpoint() else { return };
        if self.store.addon_listing != Listing::Loading {
            self.store.addon_listing = Listing::Loading;
            self.store.client.list_addons(endpoint);
        }
    }

    fn refresh_store_themes(&mut self) {
        let Some(endpoint) = self.store_endpoint() else { return };
        if self.store.theme_listing != Listing::Loading {
            self.store.theme_listing = Listing::Loading;
            self.store.client.list_themes(endpoint);
        }
    }

    pub(super) fn refresh_store(&mut self) {
        let Some(endpoint) = self.store_endpoint() else { return };
        if self.store.listing != Listing::Loading {
            self.store.listing = Listing::Loading;
            self.store.client.list(endpoint);
        }
    }

    pub(super) fn test_store_connection(&mut self) {
        match self.store_endpoint() {
            Some(endpoint) => {
                self.store.connection = Connection::Checking;
                self.store.client.check(endpoint);
            }
            None => self.notify(tr!("Set the store address and an API key first")),
        }
    }

    pub(super) fn store_settings_changed(&mut self) {
        self.store.connection = Connection::Unknown;
        self.store.store_name.clear();
        self.store.listing = Listing::NotLoaded;
        self.store.apps.clear();
        self.store.theme_listing = Listing::NotLoaded;
        self.store.themes.clear();
        self.store.addon_listing = Listing::NotLoaded;
        self.store.addons.clear();
        self.store.previews.clear();
        self.store.galleries.clear();
        if self.store_endpoint().is_some() {
            self.test_store_connection();
            self.refresh_store();
        }
    }

    pub(super) fn poll_store(&mut self) {
        for event in self.store.client.poll() {
            match event {
                Event::Checked(Ok((store, user))) => {
                    self.notify(tr!("Connected to {store} as {user}", store = store.name, user = user));
                    self.remember_store_name(&store.name);
                    self.store.connection = Connection::Ok { store: store.name, user };
                }
                Event::Checked(Err(e)) => {
                    self.notify(tr!("Store: {error}", error = e));
                    self.store.connection = Connection::Failed(e);
                }
                Event::Listed(Ok((store, apps))) => {
                    self.remember_store_name(&store.name);
                    self.store.store_name = store.name;
                    self.store.apps = apps;
                    self.store.listing = Listing::Loaded;
                }
                Event::Listed(Err(e)) => self.store.listing = Listing::Failed(e),
                Event::Icon(id, path) => {
                    self.store.icons.insert(id, path);
                }
                Event::Progress(id, done, total) => {
                    self.store.installs.insert(id, Install::Downloading(done, total));
                }
                Event::Installed(id, Ok(manifest)) => {
                    self.store.installs.remove(&id);
                    if manifest.is_addon() {
                        self.add_installed_addon(*manifest);
                    } else {
                        self.add_installed(*manifest);
                    }
                }
                Event::Imported(_, Ok(manifest)) if manifest.is_addon() => {
                    let id = manifest.id.clone();
                    if self.add_installed_addon(*manifest).is_some() {
                        self.mode = AddonManager::open(AddonView::Addon(id), 0);
                    }
                }
                Event::Imported(_, Ok(manifest)) => {
                    let i = self.add_installed(*manifest);
                    self.selected = i;
                    if matches!(self.mode, Mode::AddMenu { .. }) {
                        self.mode = Mode::Home;
                    }
                }
                Event::Imported(zip, Err(e)) => {
                    let name = zip.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    self.notify(tr!("Could not import {name}: {error}", name = name, error = e));
                }
                Event::Installed(id, Err(e)) => {
                    self.addons.resume(&id);
                    let name = self.store.package_name(&id);
                    self.notify(tr!("Could not install {name}: {error}", name = name, error = e));
                    self.store.installs.insert(id, Install::Failed(e));
                }
                Event::ThemesListed(Ok(themes)) => {
                    self.store.themes = themes;
                    self.store.theme_listing = Listing::Loaded;
                }
                Event::ThemesListed(Err(e)) => self.store.theme_listing = Listing::Failed(e),
                Event::AddonsListed(Ok(addons)) => {
                    self.store.addons = addons;
                    self.store.addon_listing = Listing::Loaded;
                }
                Event::AddonsListed(Err(e)) => self.store.addon_listing = Listing::Failed(e),
                Event::AddonIcon(id, path) => {
                    self.store.icons.insert(id, path);
                }
                Event::ThemePreview(id, i, path) => {
                    self.store.previews.insert((id, i), path);
                }
                Event::ThemeProgress(id, done, total) => {
                    self.store.theme_installs.insert(id, Install::Downloading(done, total));
                }
                Event::ThemeDownloaded(id, Ok(dir)) => {
                    self.store.theme_installs.remove(&id);
                    self.install_store_theme(&id, &dir);
                    let _ = std::fs::remove_dir_all(&dir);
                }
                Event::ThemeDownloaded(id, Err(e)) => {
                    let name = self.store.themes.iter().find(|t| t.id == id).map_or(id.clone(), |t| t.name.clone());
                    self.notify(tr!("Could not install {name}: {error}", name = name, error = e));
                    self.store.theme_installs.insert(id, Install::Failed(e));
                }
            }
        }
    }

    fn remember_store_name(&mut self, name: &str) {
        if self.store_endpoint().is_some() && self.settings.app_store().name != name {
            self.settings.app_store_mut().name = name.to_owned();
            self.save_settings();
        }
    }

    pub(super) fn import_app_file(&mut self) {
        let dialog = rfd::FileDialog::new()
            .set_title(tr!("Select a YASAFF app package (.zip with yasaffConfig.json)"))
            .add_filter(tr!("YASAFF app package"), &["zip"]);
        self.ask_path(dialog, super::Pick::File, Self::import_app_zip);
    }

    fn import_app_zip(&mut self, zip: PathBuf) {
        let busy = self.running.iter().filter_map(|r| self.library.games.iter().find(|g| g.id == r.game_id)?.app_id.clone()).collect();
        self.notify(tr!("Importing {file}…", file = zip.file_name().unwrap_or_default().to_string_lossy()));
        self.store.client.import_file(zip, busy);
    }

    fn add_installed_addon(&mut self, manifest: Manifest) -> Option<usize> {
        let placed = AddonHost::placement(&self.settings, &manifest.id).is_some();
        self.addons.reload(Some(&manifest.id));
        let (name, version) = (&manifest.name, &manifest.version);
        self.notify(if placed {
            tr!("Updated {name} {version}", name = name, version = version)
        } else {
            tr!("Installed {name} {version}. Settings → Addons places it in a bar.", name = name, version = version)
        });
        self.addons.installed.iter().position(|m| m.id == manifest.id)
    }

    pub(super) fn uninstall_addon(&mut self, i: usize) {
        let Some(addon) = self.addons.installed.get(i).cloned() else { return };
        self.addons.stop(&addon.id);
        match appstore::uninstall_addon(&addon.id) {
            Ok(()) => {
                self.settings.addons.remove(&addon.id);
                self.save_settings();
                self.addons.reload(None);
                self.notify(tr!("Uninstalled {name}. Its saved data is kept.", name = addon.name));
            }
            Err(e) => self.notify(tr!("Could not uninstall {name}: {error}", name = addon.name, error = format!("{e:#}"))),
        }
    }

    pub(super) fn choose_addon_image(&mut self, i: usize, j: usize) {
        let Some(addon) = self.addons.installed.get(i) else { return };
        let Some(image) = addon.widget_images().get(j).cloned() else { return };
        let id = addon.id.clone();
        let title = tr!("Select {name} · {image}", name = addon.name, image = image.label);
        let dialog = rfd::FileDialog::new().set_title(title).add_filter(tr!("Images"), appstore::ADDON_IMAGE_EXTS);
        self.ask_path(dialog, super::Pick::File, move |app, path| {
            let result = appstore::set_addon_image(&id, &image.key, &path);
            app.addon_images_changed(&id, result.map(|old| old.into_iter().chain(appstore::addon_image(&id, &image.key)).collect()));
        });
    }

    pub(super) fn reset_addon_images(&mut self, i: usize) {
        let Some(addon) = self.addons.installed.get(i) else { return };
        let id = addon.id.clone();
        let result = addon.widget_images().iter().try_fold(Vec::new(), |mut all, image| {
            all.extend(appstore::clear_addon_image(&id, &image.key)?);
            anyhow::Ok(all)
        });
        self.addon_images_changed(&id, result);
    }

    /// Drops the cached textures of `changed` and restarts the addon so it picks up its new images.
    fn addon_images_changed(&mut self, id: &str, changed: anyhow::Result<Vec<PathBuf>>) {
        match changed {
            Ok(paths) => {
                for path in paths {
                    self.ctx.forget_image(&file_uri(&path));
                }
                self.addons.reload(Some(id));
                self.addons.sync(&self.settings);
            }
            Err(e) => self.notify(tr!("Could not change the image: {error}", error = format!("{e:#}"))),
        }
    }

    fn installed_package(&self, app: &RemoteApp) -> Option<&Manifest> {
        if app.is_addon() { self.addons.get(&app.id) } else { self.store.installed.get(&app.id) }
    }

    fn add_installed(&mut self, manifest: Manifest) -> usize {
        self.add_to_library(manifest, true)
    }

    pub(super) fn add_to_library(&mut self, manifest: Manifest, announce: bool) -> usize {
        let id = manifest.id.clone();
        let dir = appstore::install_dir(&id);
        let run = manifest.run_entry().cloned().unwrap_or_default();
        let target = dir.join(&run.command).to_string_lossy().into_owned();
        let existing = self.library.games.iter().position(|g| g.app_id.as_deref() == Some(&id));
        let updated = existing.is_some();
        let i = existing.unwrap_or_else(|| {
            let entry = AppEntry {
                name: manifest.name.clone(),
                target: target.clone(),
                args: run.args.clone(),
                working_dir: None,
                icon: None,
                is_game: manifest.kind == "game",
                on_desktop: false,
                hidden: false,
                file: None,
            };
            self.library.add(&entry)
        });
        let game = &mut self.library.games[i];
        game.app_id = Some(id.clone());
        game.save_dir = Some(appstore::app_data_dir(&id));
        (game.target, game.args) = (target, run.args);
        let assets = [(AssetKind::Cover, manifest.icon.as_ref()), (AssetKind::Background, manifest.background.as_ref())];
        for (kind, file) in assets.into_iter().chain(manifest.artwork.assets()) {
            if let Some(file) = file
                && self.library.games[i].asset(kind).is_none()
                && let Err(e) = self.library.set_asset(i, kind, &dir.join(file))
            {
                log::warn!("{id}: {e:#}");
            }
        }
        self.save();
        let (name, version) = (&manifest.name, &manifest.version);
        if announce {
            self.notify(if updated {
                tr!("Updated {name} {version}", name = name, version = version)
            } else {
                tr!("Installed {name} {version}", name = name, version = version)
            });
        }
        self.store.installed.insert(id.clone(), manifest);
        self.library.games.iter().position(|g| g.app_id.as_deref() == Some(&id)).unwrap_or(i)
    }

    fn store_activate(&mut self, index: usize) {
        let Some(app) = self.store.packages().get(index).cloned() else { return };
        if matches!(self.store.installs.get(&app.id), Some(Install::Downloading(..))) {
            return;
        }
        if app.is_addon() {
            if self.addons.installed.iter().any(|m| m.id == app.id && !app.latest.is_update_for(m)) {
                self.mode = AddonManager::open(AddonView::Addon(app.id.clone()), 0);
                return;
            }
            let Some(endpoint) = self.store_endpoint() else { return };
            self.addons.pause(&app.id);
            self.store.installs.insert(app.id.clone(), Install::Downloading(0, app.latest.size));
            self.store.client.install(endpoint, app);
            return;
        }
        let installed = self.store.installed.get(&app.id);
        if installed.is_some_and(|m| !app.latest.is_update_for(m)) {
            if let Some(i) = self.library.games.iter().position(|g| g.app_id.as_deref() == Some(&app.id)) {
                self.selected = i;
                self.mode = Mode::Home;
                self.launch_selected();
            }
            return;
        }
        if self.library.games.iter().any(|g| g.app_id.as_deref() == Some(&app.id) && self.running_app(&g.id).is_some()) {
            self.notify(tr!("Close {name} before updating it", name = app.name));
            return;
        }
        let Some(endpoint) = self.store_endpoint() else { return };
        self.store.installs.insert(app.id.clone(), Install::Downloading(0, app.latest.size));
        self.store.client.install(endpoint, app);
    }

    pub(super) fn game_update(&self, game: &Game) -> Option<&RemoteApp> {
        self.store.update_for(game.app_id.as_deref()?)
    }

    pub(super) fn update_selected(&mut self) {
        let Some(id) = self.selected_game().and_then(|g| g.app_id.clone()) else { return };
        if let Some(i) = self.store.apps.iter().position(|a| a.id == id) {
            self.mode = Mode::Home;
            self.store_activate(i);
        }
    }

    fn store_theme_state(&self, remote: &RemoteTheme) -> ThemeState {
        let found = self.themes.iter().enumerate().find_map(|(i, t)| {
            let origin = t.settings.store.as_ref()?;
            (origin.id == remote.id).then_some((i, origin.version))
        });
        match found {
            None => ThemeState::NotInstalled,
            Some((i, version)) if version < remote.latest.version => ThemeState::Outdated(i, version),
            Some((i, _)) => ThemeState::Installed(i),
        }
    }

    fn open_store_theme(&mut self, index: usize) {
        let Some(remote) = self.store.themes.get(index).cloned() else { return };
        self.mode = Mode::StoreTheme { index, image: 0 };
        if self.store.galleries.insert(remote.id.clone())
            && let Some(endpoint) = self.store_endpoint()
        {
            self.store.client.theme_previews(endpoint, remote);
        }
    }

    fn store_theme_activate(&mut self, index: usize) {
        let Some(remote) = self.store.themes.get(index).cloned() else { return };
        if matches!(self.store.theme_installs.get(&remote.id), Some(Install::Downloading(..))) {
            return;
        }
        match self.store_theme_state(&remote) {
            ThemeState::Installed(i) => {
                let id = self.themes[i].id.clone();
                if id != self.theme().id {
                    self.select_theme(&id);
                    self.notify(tr!("{name} is now your theme. Settings → Theme switches back.", name = remote.name));
                }
            }
            ThemeState::NotInstalled | ThemeState::Outdated(..) => {
                let Some(endpoint) = self.store_endpoint() else { return };
                self.store.theme_installs.insert(remote.id.clone(), Install::Downloading(0, remote.latest.size));
                self.store.client.download_theme(endpoint, remote);
            }
        }
    }

    fn install_store_theme(&mut self, id: &str, dir: &Path) {
        let Some(remote) = self.store.themes.iter().find(|t| t.id == id).cloned() else { return };
        let origin = StoreOrigin { id: id.to_owned(), version: remote.latest.version };
        let active = self.theme().id.clone();
        let result = match self.store_theme_state(&remote) {
            ThemeState::Outdated(i, _) | ThemeState::Installed(i) => self.themes[i].update_from_store(dir, origin).map(|()| true),
            ThemeState::NotInstalled => Theme::install_from_store(dir, origin).map(|theme| {
                self.themes.push(theme);
                false
            }),
        };
        match result {
            Ok(updated) => {
                self.select_theme(&active);
                let (name, version) = (&remote.name, remote.latest.version);
                self.notify(if updated {
                    tr!("Updated {name} {version}", name = name, version = version)
                } else {
                    tr!("Installed {name} {version}", name = name, version = version)
                });
            }
            Err(e) => {
                self.notify(tr!("Could not install {name}: {error}", name = remote.name, error = format!("{e:#}")));
                self.store.theme_installs.insert(id.to_owned(), Install::Failed(format!("{e:#}")));
            }
        }
    }

    /// Whether a store download or install is in progress.
    pub(super) fn store_busy(&self) -> bool {
        self.store.installs.values().chain(self.store.theme_installs.values()).any(|i| matches!(i, Install::Downloading(..)))
    }

    pub(super) fn running_app(&self, game_id: &str) -> Option<&RunningApp> {
        self.running.iter().find(|r| r.game_id == game_id)
    }

    pub(super) fn app_running_at(&self, i: usize) -> bool {
        self.library.games.get(i).is_some_and(|g| self.running_app(&g.id).is_some())
    }

    pub(super) fn front_app_index(&self) -> Option<usize> {
        self.library.position(&self.running.last()?.game_id)
    }

    pub(super) fn launch_app(&mut self) {
        let Some(game) = self.selected_game() else { return };
        let Some(app_id) = game.app_id.clone() else { return };
        if self.running_app(&game.id).is_some() {
            self.switch_to_running();
            return;
        }
        let (name, cover, font, game_id) = (game.name.clone(), game.asset(AssetKind::Cover).map(Path::to_path_buf), self.game_font(game), game.id.clone());
        match appstore::launch(&self.ctx, &app_id, Placement::of(&self.ctx, self.fullscreen)) {
            Ok(process) => {
                let pid = process.child.id();
                self.running.push(RunningApp { game_id, name: name.clone(), process, started: Instant::now() });
                self.selected = self.library.mark_played(self.selected, self.settings.order.moves_played());
                self.save();
                let watch = GameWatch::start(self.ctx.clone(), Some(pid));
                self.focus_guard_until = None;
                self.mode = Mode::Launching(Launching { name, font, cover, via: None, store_app: true, started: Instant::now(), watch });
            }
            Err(e) => self.notify(tr!("Could not start {name}: {error}", name = name, error = format!("{e:#}"))),
        }
    }

    pub(super) fn switch_to_running(&mut self) {
        self.mode = Mode::Home;
        self.focus_guard_until = None;
        let Some(id) = self.selected_game().map(|g| g.id.clone()) else { return };
        let Some(at) = self.running.iter().position(|r| r.game_id == id) else { return };
        let running = self.running.remove(at);
        running.process.show(Placement::of(&self.ctx, self.fullscreen));
        self.running.push(running);
    }

    pub(super) fn close_running(&mut self) {
        let id = self.selected_game().map(|g| g.id.clone());
        if let Some(running) = self.running.iter_mut().find(|r| Some(&r.game_id) == id.as_ref()) {
            let _ = running.process.child.kill();
        }
        self.mode = Mode::Home;
    }

    pub(super) fn poll_running(&mut self, ctx: &Context) {
        let mut ended = Vec::new();
        let mut i = 0;
        while i < self.running.len() {
            let running = &mut self.running[i];
            if running.process.wants_home() {
                ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
            match running.process.child.try_wait() {
                Ok(Some(status)) => ended.push((self.running.remove(i), status)),
                Ok(None) => i += 1,
                Err(e) => {
                    log::warn!("{}: {e}", running.name);
                    i += 1;
                }
            }
        }
        for (running, status) in ended {
            if matches!(self.mode, Mode::Launching(_)) {
                self.mode = Mode::Home;
            }
            if let Some(i) = self.library.position(&running.game_id) {
                self.selected = i;
            }
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            if !status.success() && running.started.elapsed() < QUICK_EXIT {
                self.notify(tr!("{name} closed right after starting ({status})", name = running.name, status = status));
            }
        }
    }

    pub(super) fn uninstall_selected(&mut self) {
        let Some(game) = self.selected_game() else { return };
        let Some(id) = game.app_id.clone() else { return };
        if self.app_running_at(self.selected) {
            self.notify(tr!("Close {name} before uninstalling it", name = game.name));
            return;
        }
        let name = game.name.clone();
        match appstore::uninstall(&id) {
            Ok(()) => {
                self.store.installed.remove(&id);
                self.remove_selected();
                self.notify(tr!("Uninstalled {name}. Its saved data is kept.", name = name));
            }
            Err(e) => self.notify(tr!("Could not uninstall {name}: {error}", name = name, error = format!("{e:#}"))),
        }
    }

    pub(super) fn store_status(&self) -> (String, Color32) {
        let gray = Color32::from_gray(140);
        match &self.store.connection {
            _ if self.store_endpoint().is_none() => (tr!("Not set up").to_owned(), gray),
            Connection::Unknown => (tr!("Not tested").to_owned(), gray),
            Connection::Checking => (tr!("Connecting…").to_owned(), gray),
            Connection::Ok { store, user } => (tr!("Connected to {store} as {user}", store = store, user = user), PAD_A),
            Connection::Failed(e) => (e.clone(), PAD_B),
        }
    }

    pub(super) fn masked_api_key(&self) -> String {
        let key = self.settings.app_store().api_key.trim();
        match key.char_indices().nth(8) {
            _ if key.is_empty() => tr!("Not set").to_owned(),
            Some((i, _)) => format!("{}…", &key[..i]),
            None => tr!("Set").to_owned(),
        }
    }

    pub(super) fn handle_store(&mut self, action: Action) {
        let connected = self.store_endpoint().is_some();
        let Mode::Store { cursor, .. } = &mut self.mode else { return };
        let count = self.store.count();
        match action {
            Action::Up | Action::Left => *cursor = cursor.saturating_sub(1),
            Action::Down | Action::Right => *cursor = (*cursor + 1).min(count.saturating_sub(1)),
            Action::Confirm if !connected => self.mode = self.settings_at(SettingRow::AppStoreUrl),
            Action::Confirm => {
                let i = *cursor;
                match self.store.tab {
                    StoreTab::Apps | StoreTab::Addons => self.store_activate(i),
                    StoreTab::Themes => self.open_store_theme(i),
                }
            }
            Action::Options => self.switch_store_tab(),
            Action::Add => match self.store.tab {
                StoreTab::Apps => {
                    self.store.listing = Listing::NotLoaded;
                    self.refresh_store();
                }
                StoreTab::Addons => {
                    self.store.addon_listing = Listing::NotLoaded;
                    self.refresh_store_addons();
                }
                StoreTab::Themes => {
                    self.store.theme_listing = Listing::NotLoaded;
                    self.store.galleries.clear();
                    self.refresh_store_themes();
                }
            },
            Action::Back => self.mode = Mode::Home,
            _ => {}
        }
    }

    pub(super) fn store_on_themes(&self) -> bool {
        self.store.tab == StoreTab::Themes
    }

    fn switch_store_tab(&mut self) {
        self.store.tab = cycle(&StoreTab::ALL, self.store.tab, 1);
        self.open_store_tab(self.store.tab);
    }

    fn open_store_tab(&mut self, tab: StoreTab) {
        self.store.tab = tab;
        self.mode = Mode::Store { cursor: 0, scroll: 0 };
        self.load_store_tab();
    }

    pub(super) fn handle_store_theme(&mut self, action: Action) {
        let Mode::StoreTheme { index, image } = &mut self.mode else { return };
        let images = self.store.themes.get(*index).map_or(0, |t| t.preview_urls.len());
        match action {
            Action::Left => *image = image.checked_sub(1).unwrap_or(images.saturating_sub(1)),
            Action::Right => *image = if *image + 1 >= images { 0 } else { *image + 1 },
            Action::Confirm => {
                let i = *index;
                self.store_theme_activate(i);
            }
            Action::Back => self.mode = Mode::Store { cursor: *index, scroll: 0 },
            _ => {}
        }
    }

    pub(super) fn draw_store(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::Store { mut cursor, mut scroll } = self.mode else { return };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(170));
        let panel = store_panel(screen, s);
        painter.rect_filled(panel, (8.0 * s) as u8, Color32::from_rgb(14, 20, 36));
        let title = if self.store.store_name.is_empty() { tr!("YASAFF Store").to_owned() } else { self.store.store_name.clone() };
        let title = painter.text(panel.left_top() + vec2(40.0 * s, 36.0 * s), Align2::LEFT_TOP, title, FontId::proportional(30.0 * s), Color32::WHITE);
        let switch_tab = self.draw_store_tabs(ui, pos2(title.right() + 36.0 * s, title.center().y), s);
        let count = self.store.count();
        let counts = match self.store.tab {
            _ if *self.store.listing() != Listing::Loaded => String::new(),
            StoreTab::Apps => tr_n!(count, "{n} app for {platform}", "{n} apps for {platform}", platform = appstore::platform()),
            StoreTab::Themes => tr_n!(count, "{n} theme", "{n} themes"),
            StoreTab::Addons => tr_n!(count, "{n} addon for {platform}", "{n} addons for {platform}", platform = appstore::platform()),
        };
        painter.text(panel.right_top() + vec2(-40.0 * s, 48.0 * s), Align2::RIGHT_CENTER, counts, FontId::proportional(18.0 * s), Color32::from_gray(150));
        if let Some(tab) = switch_tab {
            self.open_store_tab(tab);
            return;
        }

        let body = Rect::from_min_max(panel.left_top() + vec2(0.0, 100.0 * s), panel.max - vec2(0.0, 24.0 * s));
        let center_text = |title: &str, sub: &str, color: Color32| {
            painter.text(body.center() - vec2(0.0, 20.0 * s), Align2::CENTER_CENTER, title, FontId::proportional(26.0 * s), color);
            let galley = painter.layout(sub.to_owned(), FontId::proportional(18.0 * s), Color32::from_gray(160), body.width() * 0.7);
            painter.galley(body.center() + vec2(-galley.size().x / 2.0, 14.0 * s), galley, Color32::from_gray(160));
        };
        if self.store_endpoint().is_none() {
            center_text(
                tr!("Connect a store"),
                tr!("Settings → YASAFF Store: paste the store's address and an API key from its API keys page. Press A to open it."),
                Color32::WHITE,
            );
            return;
        }
        match self.store.listing() {
            Listing::Loading | Listing::NotLoaded if count == 0 => {
                egui::Spinner::new().size(48.0 * s).color(Color32::WHITE).paint_at(ui, Rect::from_center_size(body.center(), vec2(48.0 * s, 48.0 * s)));
                return;
            }
            Listing::Failed(e) if count == 0 => {
                center_text(
                    tr!("Could not load the store"),
                    &tr!("{error}\n\nY to try again · Settings → YASAFF Store to change the address or key", error = e),
                    PAD_B,
                );
                return;
            }
            Listing::Loaded if count == 0 => {
                let sub = match self.store.tab {
                    StoreTab::Apps => tr!("This store has no apps for {platform} yet.", platform = appstore::platform()),
                    StoreTab::Themes => tr!("This store has no themes yet.").to_owned(),
                    StoreTab::Addons => tr!("This store has no addons for {platform} yet.", platform = appstore::platform()),
                };
                center_text(tr!("Nothing here yet"), &sub, Color32::WHITE);
                return;
            }
            _ => {}
        }

        let list = Rect::from_min_max(body.min + vec2(20.0 * s, 0.0), pos2(body.left() + body.width() * 0.46, body.bottom()));
        let row_h = 96.0 * s;
        let rows = ((list.height() / row_h).floor() as usize).max(1);
        let wheel = wheel_lines(ui);
        if wheel.abs() >= 0.5 {
            cursor = cursor.saturating_add_signed(-(wheel.round().clamp(-5.0, 5.0) as isize));
        }
        cursor = cursor.min(count - 1);
        if cursor < scroll {
            scroll = cursor;
        } else if cursor >= scroll + rows {
            scroll = cursor + 1 - rows;
        }
        scroll = scroll.min(count.saturating_sub(rows));

        let mut clicked = None;
        for i in (scroll..count).take(rows) {
            let r = Rect::from_min_size(pos2(list.left(), list.top() + (i - scroll) as f32 * row_h), vec2(list.width(), row_h - 8.0 * s));
            if i == cursor {
                painter.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(22));
                painter.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
            }
            match self.store.tab {
                StoreTab::Apps | StoreTab::Addons => self.paint_store_app_row(ui, &self.store.packages()[i], r, s),
                StoreTab::Themes => self.paint_store_theme_row(ui, &self.store.themes[i], r, s),
            }
            let resp = ui.interact(r, Id::new(("store-row", i)), Sense::click());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                cursor = i;
            }
            if resp.clicked() {
                clicked = Some(i);
            }
        }
        if count > rows {
            painter.text(list.center_bottom() + vec2(0.0, 6.0 * s), Align2::CENTER_TOP, format!("{} / {count}", cursor + 1), FontId::proportional(14.0 * s), Color32::from_gray(120));
        }

        let detail = Rect::from_min_max(pos2(list.right() + 40.0 * s, body.top()), body.max - vec2(40.0 * s, 0.0));
        let button = match self.store.tab {
            StoreTab::Apps | StoreTab::Addons => self.draw_store_detail(ui, cursor, detail, s),
            StoreTab::Themes => self.draw_store_theme_summary(ui, cursor, detail, s),
        };

        self.mode = Mode::Store { cursor, scroll };
        let activate = |app: &mut Self, i: usize| match app.store.tab {
            StoreTab::Apps | StoreTab::Addons => app.store_activate(i),
            StoreTab::Themes => app.open_store_theme(i),
        };
        if let Some(i) = clicked {
            if i == cursor {
                activate(self, i);
            } else if let Mode::Store { cursor, .. } = &mut self.mode {
                *cursor = i;
            }
        } else if button {
            activate(self, cursor);
        }
    }

    fn draw_store_tabs(&self, ui: &Ui, left_center: Pos2, s: f32) -> Option<StoreTab> {
        let painter = ui.painter();
        let mut x = left_center.x;
        let mut clicked = None;
        for (i, tab) in StoreTab::ALL.into_iter().enumerate() {
            let on = self.store.tab == tab;
            let galley = painter.layout_no_wrap(tab.label().to_owned(), FontId::proportional(20.0 * s), Color32::WHITE);
            let pill = Rect::from_min_size(pos2(x, left_center.y - 18.0 * s), vec2(galley.size().x + 32.0 * s, 36.0 * s));
            painter.rect_filled(pill, (18.0 * s) as u8, if on { ACCENT } else { Color32::from_white_alpha(14) });
            painter.galley(pill.center() - galley.size() / 2.0, galley, if on { Color32::WHITE } else { Color32::from_gray(170) });
            if !on && ui.interact(pill, Id::new(("store-tab", i)), Sense::click()).clicked() {
                clicked = Some(tab);
            }
            x = pill.right() + 10.0 * s;
        }
        painter.text(pos2(x + 8.0 * s, left_center.y), Align2::LEFT_CENTER, tr!("Menu / Tab"), FontId::proportional(14.0 * s), Color32::from_gray(110));
        clicked
    }

    fn paint_store_app_row(&self, ui: &Ui, app: &RemoteApp, r: Rect, s: f32) {
        let painter = ui.painter();
        let icon = Rect::from_min_size(r.left_top() + vec2(20.0 * s, 8.0 * s), vec2(r.height() - 16.0 * s, r.height() - 16.0 * s));
        self.paint_store_icon(ui, app, icon, s);
        let (label, color) = self.store_badge(app);
        let text_x = icon.right() + 18.0 * s;
        let progress = match self.store.installs.get(&app.id) {
            Some(Install::Downloading(done, total)) => Some(percent(*done, *total)),
            _ => None,
        };
        paint_row_text(painter, r, text_x, &app.name, &app.summary, &label, color, progress, s);
    }

    fn paint_store_theme_row(&self, ui: &Ui, theme: &RemoteTheme, r: Rect, s: f32) {
        let thumb = Rect::from_min_size(r.left_top() + vec2(20.0 * s, 8.0 * s), vec2((r.height() - 16.0 * s) * 16.0 / 9.0, r.height() - 16.0 * s));
        self.paint_theme_preview(ui, theme, 0, thumb, s);
        let (label, color) = self.store_theme_badge(theme);
        let progress = match self.store.theme_installs.get(&theme.id) {
            Some(Install::Downloading(done, total)) => Some(percent(*done, *total)),
            _ => None,
        };
        let summary = if theme.summary.is_empty() { tr!("by {author}", author = theme.author) } else { theme.summary.clone() };
        paint_row_text(ui.painter(), r, thumb.right() + 18.0 * s, &theme.name, &summary, &label, color, progress, s);
    }

    fn draw_store_theme_summary(&self, ui: &Ui, index: usize, rect: Rect, s: f32) -> bool {
        let Some(theme) = self.store.themes.get(index) else { return false };
        let painter = ui.painter().with_clip_rect(rect);
        let image = Rect::from_min_size(rect.min, vec2(rect.width(), rect.width() * 9.0 / 16.0).min(vec2(rect.width(), rect.height() * 0.55)));
        self.paint_theme_preview(ui, theme, 0, image, s);
        let mut y = image.bottom() + 24.0 * s;
        painter.text(pos2(rect.left(), y), Align2::LEFT_TOP, &theme.name, FontId::proportional(34.0 * s), Color32::WHITE);
        y += 48.0 * s;
        painter.text(pos2(rect.left(), y), Align2::LEFT_TOP, self.store_theme_facts(theme), FontId::proportional(17.0 * s), Color32::from_gray(160));
        y += 36.0 * s;
        let btn = Rect::from_min_size(pos2(rect.left(), y), vec2(260.0 * s, 56.0 * s));
        painter.rect_filled(btn, (4.0 * s) as u8, Color32::WHITE);
        painter.text(btn.center(), Align2::CENTER_CENTER, tr!("See gallery"), FontId::proportional(24.0 * s), Color32::BLACK);
        let clicked = ui.interact(btn, Id::new("store-theme-open"), Sense::click()).clicked();
        y = btn.bottom() + 24.0 * s;
        let text = if theme.description.is_empty() { &theme.summary } else { &theme.description };
        let galley = painter.layout(text.to_owned(), FontId::proportional(18.0 * s), Color32::from_gray(200), rect.width());
        painter.galley(pos2(rect.left(), y), galley, Color32::from_gray(200));
        clicked
    }

    pub(super) fn draw_store_theme(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::StoreTheme { index, mut image } = self.mode else { return };
        let Some(theme) = self.store.themes.get(index) else {
            self.mode = Mode::Store { cursor: 0, scroll: 0 };
            return;
        };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(170));
        let panel = store_panel(screen, s);
        painter.rect_filled(panel, (8.0 * s) as u8, Color32::from_rgb(14, 20, 36));
        let inner = panel.shrink(40.0 * s);
        let images = theme.preview_urls.len();
        image = image.min(images.saturating_sub(1));

        let gallery_w = inner.width() * 0.64;
        let strip_h = if images > 1 { 96.0 * s } else { 0.0 };
        let main_h = (gallery_w * 9.0 / 16.0).min(inner.height() - strip_h - 20.0 * s);
        let main = Rect::from_min_size(inner.min, vec2(gallery_w, main_h));
        painter.rect_filled(main, (6.0 * s) as u8, Color32::from_rgb(5, 8, 15));
        match self.store.previews.get(&(theme.id.clone(), image)) {
            Some(path) => paint_image_fit(ui, path, main, ImageFit::Fit),
            None if images == 0 => {
                painter.text(main.center(), Align2::CENTER_CENTER, tr!("No preview images"), FontId::proportional(22.0 * s), Color32::from_gray(140));
            }
            None => egui::Spinner::new().size(40.0 * s).color(Color32::WHITE).paint_at(ui, Rect::from_center_size(main.center(), vec2(40.0 * s, 40.0 * s))),
        }
        let mut picked = None;
        if images > 1 {
            painter.text(main.right_bottom() + vec2(-12.0 * s, -10.0 * s), Align2::RIGHT_BOTTOM, format!("◀ {} / {images} ▶", image + 1), FontId::proportional(16.0 * s), Color32::WHITE);
            let thumb_w = (strip_h - 12.0 * s) * 16.0 / 9.0;
            let fits = ((gallery_w + 10.0 * s) / (thumb_w + 10.0 * s)).floor().max(1.0) as usize;
            let first = image.saturating_sub(fits - 1).min(images.saturating_sub(fits));
            for i in (first..images).take(fits) {
                let t = Rect::from_min_size(pos2(main.left() + (i - first) as f32 * (thumb_w + 10.0 * s), main.bottom() + 16.0 * s), vec2(thumb_w, strip_h - 12.0 * s));
                self.paint_theme_preview(ui, theme, i, t, s);
                if i == image {
                    painter.rect_stroke(t.expand(3.0 * s), (4.0 * s) as u8, Stroke::new(3.0 * s, ACCENT), StrokeKind::Outside);
                }
                if ui.interact(t, Id::new(("theme-thumb", i)), Sense::click()).clicked() {
                    picked = Some(i);
                }
            }
        }

        let side = Rect::from_min_max(pos2(main.right() + 40.0 * s, inner.top()), inner.max);
        let side_painter = painter.with_clip_rect(side);
        let mut y = side.top();
        let title = side_painter.layout(theme.name.clone(), FontId::proportional(36.0 * s), Color32::WHITE, side.width());
        let h = title.size().y;
        side_painter.galley(pos2(side.left(), y), title, Color32::WHITE);
        y += h + 10.0 * s;
        side_painter.text(pos2(side.left(), y), Align2::LEFT_TOP, self.store_theme_facts(theme), FontId::proportional(17.0 * s), Color32::from_gray(160));
        y += 30.0 * s;
        let state = self.store_theme_state(theme);
        let installing = match self.store.theme_installs.get(&theme.id) {
            Some(Install::Downloading(done, total)) => Some(percent(*done, *total)),
            _ => None,
        };
        let status = match (self.store.theme_installs.get(&theme.id), state) {
            (Some(Install::Failed(e)), _) => (tr!("Install failed: {error}", error = e), PAD_B),
            (_, ThemeState::Outdated(_, v)) => (tr!("Version {old} installed · {new} available", old = v, new = theme.latest.version), ACCENT),
            (_, ThemeState::Installed(i)) if i == self.theme => (tr!("Installed · your current theme").to_owned(), PAD_A),
            (_, ThemeState::Installed(_)) => (tr!("Installed").to_owned(), PAD_A),
            _ => (tr_n!(theme.downloads, "{n} download", "{n} downloads"), Color32::from_gray(160)),
        };
        let status_galley = side_painter.layout(status.0, FontId::proportional(17.0 * s), status.1, side.width());
        let h = status_galley.size().y;
        side_painter.galley(pos2(side.left(), y), status_galley, status.1);
        y += h + 24.0 * s;

        let (label, enabled) = match (installing, state) {
            (Some(p), _) => (tr!("Installing… {percent} %", percent = p), false),
            (_, ThemeState::NotInstalled) => (tr!("Install").to_owned(), true),
            (_, ThemeState::Outdated(..)) => (tr!("Update to version {version}", version = theme.latest.version), true),
            (_, ThemeState::Installed(i)) if i == self.theme => (tr!("In use").to_owned(), false),
            (_, ThemeState::Installed(_)) => (tr!("Use this theme").to_owned(), true),
        };
        let btn = Rect::from_min_size(pos2(side.left(), y), vec2(side.width().min(320.0 * s), 60.0 * s));
        side_painter.rect_filled(btn, (4.0 * s) as u8, if enabled { Color32::WHITE } else { Color32::from_gray(90) });
        side_painter.text(btn.center(), Align2::CENTER_CENTER, label, FontId::proportional(24.0 * s), Color32::BLACK);
        let pressed = enabled && ui.interact(btn, Id::new("store-theme-button"), Sense::click()).clicked();
        y = btn.bottom() + 30.0 * s;

        let mut paragraph = |text: &str, size: f32, color: Color32, gap: f32| {
            let galley = side_painter.layout(text.to_owned(), FontId::proportional(size * s), color, side.width());
            let h = galley.size().y;
            side_painter.galley(pos2(side.left(), y), galley, color);
            y += h + gap * s;
        };
        let description = if theme.description.is_empty() { &theme.summary } else { &theme.description };
        if !description.is_empty() {
            paragraph(description, 18.0, Color32::from_gray(215), 22.0);
        }
        if let Some(notes) = theme.latest.notes.as_deref().filter(|n| !n.is_empty()) {
            paragraph(&tr!("What's new in version {version}", version = theme.latest.version), 17.0, ACCENT, 6.0);
            paragraph(notes, 16.0, Color32::from_gray(190), 0.0);
        }

        self.mode = Mode::StoreTheme { index, image: picked.unwrap_or(image) };
        if pressed {
            self.store_theme_activate(index);
        }
    }

    fn store_theme_facts(&self, theme: &RemoteTheme) -> String {
        let author = if theme.author.is_empty() { tr!("unknown author") } else { &theme.author };
        let size = theme.latest.size as f64 / (1024.0 * 1024.0);
        tr!("by {author} · version {version} · {size} MB", author = author, version = theme.latest.version, size = format!("{size:.1}"))
    }

    fn store_theme_badge(&self, theme: &RemoteTheme) -> (String, Color32) {
        if let Some(install) = self.store.theme_installs.get(&theme.id) {
            return match install {
                Install::Downloading(done, total) => (format!("{} %", percent(*done, *total)), Color32::from_gray(200)),
                Install::Failed(_) => (tr!("Failed").to_owned(), PAD_B),
            };
        }
        match self.store_theme_state(theme) {
            ThemeState::NotInstalled => (tr!("Get").to_owned(), Color32::WHITE),
            ThemeState::Outdated(..) => (tr!("Update").to_owned(), ACCENT),
            ThemeState::Installed(i) if i == self.theme => (tr!("In use").to_owned(), PAD_A),
            ThemeState::Installed(_) => (tr!("Installed").to_owned(), PAD_A),
        }
    }

    fn paint_theme_preview(&self, ui: &Ui, theme: &RemoteTheme, i: usize, rect: Rect, s: f32) {
        let radius = (4.0 * s) as u8;
        match self.store.previews.get(&(theme.id.clone(), i)) {
            Some(path) => paint_image_cover(ui, path, rect, 1.0, radius),
            None => {
                ui.painter().rect_filled(rect, radius, placeholder_color(&theme.name));
                if i == 0 && theme.preview_urls.is_empty() {
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, initials(&theme.name), FontId::proportional(rect.height() * 0.3), Color32::WHITE);
                }
            }
        }
    }

    fn draw_store_detail(&self, ui: &Ui, index: usize, rect: Rect, s: f32) -> bool {
        let Some(app) = self.store.packages().get(index) else { return false };
        let painter = ui.painter().with_clip_rect(rect);
        let icon = Rect::from_min_size(rect.min, vec2(200.0 * s, 200.0 * s));
        self.paint_store_icon(ui, app, icon, s);
        let x = icon.right() + 30.0 * s;
        let mut y = icon.top() + 6.0 * s;
        painter.text(pos2(x, y), Align2::LEFT_TOP, &app.name, FontId::proportional(38.0 * s), Color32::WHITE);
        y += 54.0 * s;
        let size = app.latest.size as f64 / (1024.0 * 1024.0);
        let author = if app.author.is_empty() { tr!("unknown author") } else { &app.author };
        let kind = if app.is_addon() { tr!("bar widget") } else { &app.kind };
        let facts = tr!(
            "{author} · {kind} · version {version} · {size} MB",
            author = author,
            kind = kind,
            version = app.latest.version,
            size = format!("{size:.1}"),
        );
        painter.text(pos2(x, y), Align2::LEFT_TOP, facts, FontId::proportional(17.0 * s), Color32::from_gray(160));
        y += 30.0 * s;
        if let Some(min) = app.min_height {
            let text = tr!("Needs a bar at least {px} px high; a lower bar is raised", px = format!("{min:.0}"));
            painter.text(pos2(x, y), Align2::LEFT_TOP, text, FontId::proportional(17.0 * s), PAD_Y);
            y += 30.0 * s;
        }
        let installed = self.installed_package(app);
        let status = match (self.store.installs.get(&app.id), installed) {
            (Some(Install::Failed(e)), _) => (tr!("Install failed: {error}", error = e), PAD_B),
            (_, Some(m)) if app.latest.is_update_for(m) => {
                (tr!("Version {old} installed · {new} available", old = m.version, new = app.latest.version), ACCENT)
            }
            (_, Some(m)) => (tr!("Installed · version {version}", version = m.version), PAD_A),
            _ => (tr_n!(app.downloads, "{n} download", "{n} downloads"), Color32::from_gray(160)),
        };
        painter.text(pos2(x, y), Align2::LEFT_TOP, status.0, FontId::proportional(17.0 * s), status.1);

        let label = match (self.store.installs.get(&app.id), installed) {
            (Some(Install::Downloading(done, total)), _) => {
                tr!("Installing… {percent} %", percent = percent(*done, *total))
            }
            (_, Some(m)) if app.latest.is_update_for(m) => tr!("Update to {version}", version = app.latest.version),
            (_, Some(_)) if app.is_addon() => tr!("Place in a bar").to_owned(),
            (_, Some(_)) => tr!("Start").to_owned(),
            _ => tr!("Get").to_owned(),
        };
        let btn = Rect::from_min_size(pos2(x, icon.bottom() - 56.0 * s), vec2(260.0 * s, 56.0 * s));
        painter.rect_filled(btn, (4.0 * s) as u8, Color32::WHITE);
        painter.text(btn.center(), Align2::CENTER_CENTER, label, FontId::proportional(24.0 * s), Color32::BLACK);
        let clicked = ui.interact(btn, Id::new("store-button"), Sense::click()).clicked();

        let mut y = icon.bottom() + 36.0 * s;
        let mut paragraph = |text: &str, size: f32, color: Color32, gap: f32| {
            let galley = painter.layout(text.to_owned(), FontId::proportional(size * s), color, rect.width());
            let h = galley.size().y;
            painter.galley(pos2(rect.left(), y), galley, color);
            y += h + gap * s;
        };
        let description = if app.description.is_empty() { &app.summary } else { &app.description };
        paragraph(description, 19.0, Color32::from_gray(215), 24.0);
        if let Some(notes) = app.latest.notes.as_deref().filter(|n| !n.is_empty()) {
            paragraph(&tr!("What's new in {version}", version = app.latest.version), 18.0, ACCENT, 6.0);
            paragraph(notes, 17.0, Color32::from_gray(190), 24.0);
        }
        if let Some(git) = &app.latest.git {
            let mut source = Vec::new();
            if let Some(commit) = &git.commit {
                let short = &commit[..commit.len().min(10)];
                source.push(match &git.branch {
                    Some(branch) => tr!("Built from {commit} on {branch}", commit = short, branch = branch),
                    None => tr!("Built from {commit}", commit = short),
                });
            }
            if let Some(repo) = &git.repository {
                source.push(repo.clone());
            }
            paragraph(&source.join(" · "), 15.0, Color32::from_gray(130), 0.0);
        }
        clicked
    }

    fn store_badge(&self, app: &RemoteApp) -> (String, Color32) {
        match (self.store.installs.get(&app.id), self.installed_package(app)) {
            (Some(Install::Downloading(done, total)), _) => {
                (format!("{} %", percent(*done, *total)), Color32::from_gray(200))
            }
            (Some(Install::Failed(_)), _) => (tr!("Failed").to_owned(), PAD_B),
            (_, Some(m)) if app.latest.is_update_for(m) => (tr!("Update").to_owned(), ACCENT),
            (_, Some(_)) => (tr!("Installed").to_owned(), PAD_A),
            _ => (tr!("Get").to_owned(), Color32::WHITE),
        }
    }

    fn paint_store_icon(&self, ui: &Ui, app: &RemoteApp, rect: Rect, s: f32) {
        let radius = (6.0 * s) as u8;
        match self.store.icons.get(&app.id) {
            Some(path) => paint_image_cover(ui, path, rect, 1.0, radius),
            None => {
                ui.painter().rect_filled(rect, radius, placeholder_color(&app.name));
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, initials(&app.name), FontId::proportional(rect.height() * 0.3), Color32::WHITE);
            }
        }
    }

    pub(super) fn paint_running_glow(&self, ui: &Ui, i: usize, rect: Rect, s: f32) {
        if !self.is_active(i) {
            return;
        }
        const RINGS: usize = 10;
        let pulse = 0.75 + 0.25 * (ui.input(|inp| inp.time) as f32 * 2.5).sin();
        let step = (rect.height() * 0.012).clamp(2.0 * s, 5.0 * s);
        let painter = ui.painter();
        for ring in 0..RINGS {
            let fade = 1.0 - ring as f32 / RINGS as f32;
            let color = PAD_A.gamma_multiply(0.55 * pulse * fade * fade);
            let r = rect.expand(step * (ring as f32 + 0.5));
            painter.rect_stroke(r, (8.0 * s + step * ring as f32) as u8, Stroke::new(step, color), StrokeKind::Middle);
        }
    }

    pub(super) fn paint_app_badges(&self, ui: &Ui, i: usize, rect: Rect, s: f32) {
        let Some(game) = self.library.games.get(i) else { return };
        let running = self.is_active(i);
        if game.app_id.is_none() && !running {
            return;
        }
        let painter = ui.painter();
        let font = FontId::proportional((rect.height() * 0.08).clamp(11.0 * s, 17.0 * s));
        let mut x = rect.left() + 8.0 * s;
        let update = self.game_update(game).is_some();
        for (on, label, color) in [(running, tr!("▶ Running"), PAD_A), (update, tr!("Update"), ACCENT)] {
            if !on {
                continue;
            }
            let galley = painter.layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE);
            let pill = Rect::from_min_size(pos2(x, rect.top() + 8.0 * s), galley.size() + vec2(14.0 * s, 6.0 * s));
            painter.rect_filled(pill, (pill.height() / 2.0) as u8, color);
            painter.galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
            x = pill.right() + 6.0 * s;
        }
    }
}

fn store_panel(screen: Rect, s: f32) -> Rect {
    Rect::from_center_size(screen.center(), vec2((1500.0 * s).min(screen.width() - 40.0), screen.height() - 150.0 * s))
}

#[allow(clippy::too_many_arguments)]
fn paint_row_text(painter: &Painter, r: Rect, text_x: f32, name: &str, summary: &str, label: &str, color: Color32, progress: Option<u64>, s: f32) {
    let tag = painter.layout_no_wrap(label.to_owned(), FontId::proportional(15.0 * s), Color32::BLACK);
    let t = Rect::from_min_size(pos2(r.right() - 20.0 * s - tag.size().x - 20.0 * s, r.center().y - 13.0 * s), tag.size() + vec2(20.0 * s, 10.0 * s));
    let clip = painter.with_clip_rect(Rect::from_min_max(r.min, pos2(t.left() - 10.0 * s, r.bottom())));
    clip.text(pos2(text_x, r.top() + 16.0 * s), Align2::LEFT_TOP, name, FontId::proportional(23.0 * s), Color32::WHITE);
    clip.text(pos2(text_x, r.top() + 50.0 * s), Align2::LEFT_TOP, summary, FontId::proportional(16.0 * s), Color32::from_gray(150));
    painter.rect_filled(t, (t.height() / 2.0) as u8, color);
    painter.galley(t.center() - tag.size() / 2.0, tag, Color32::BLACK);
    if let Some(p) = progress {
        let bar = Rect::from_min_size(pos2(text_x, r.bottom() - 10.0 * s), vec2(r.right() - 20.0 * s - text_x, 4.0 * s));
        painter.rect_filled(bar, 2, Color32::from_white_alpha(30));
        painter.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * (p as f32 / 100.0).clamp(0.0, 1.0), bar.height())), 2, ACCENT);
    }
}

fn percent(done: u64, total: u64) -> u64 {
    (done * 100).checked_div(total).unwrap_or(0).min(100)
}
