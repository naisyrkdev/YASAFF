//! A game's SD and USB settings, and copying its saves to the USB stick or SD card it was
//! started from once it closes.

use super::*;
use crate::device_saves;

const DEVICE_REFRESH: Duration = Duration::from_secs(1);

pub(super) struct DeviceSync {
    game: String,
    device: String,
    job: SyncJob,
    rx: mpsc::Receiver<SyncMsg>,
    action: String,
    result: Option<Result<PathBuf, String>>,
}

#[derive(Clone)]
struct SyncJob {
    game: String,
    device_root: PathBuf,
    paths: Vec<PathBuf>,
    recovery: u32,
}

enum SyncMsg {
    Action(String),
    Done(Result<PathBuf, String>),
}

/// What gets copied to the device: the files and folders picked for it, else the save folder.
pub(super) fn device_save_paths(game: &Game) -> Vec<PathBuf> {
    match &game.device_saves.paths[..] {
        [] => game.save_dir.iter().cloned().collect(),
        paths => paths.to_vec(),
    }
}

impl App {
    pub(super) fn open_device_settings(&mut self) {
        self.devices_looked = None;
        self.refresh_devices();
        self.mode = Mode::Settings { cursor: 0, page: Page::GameDevice, in_rows: false };
    }

    pub(super) fn refresh_devices(&mut self) {
        let on_page = matches!(self.mode, Mode::Settings { page: Page::GameDevice, .. });
        if self.devices_looked.is_some_and(|t| !on_page || t.elapsed() < DEVICE_REFRESH) {
            return;
        }
        self.devices = crate::physical::removable_volumes();
        self.devices_looked = Some(Instant::now());
    }

    /// The device the SD and USB page acts on.
    pub(super) fn picked_device(&self) -> Option<&Volume> {
        let pick = self.device_pick.as_ref();
        self.devices.iter().find(|v| Some(&v.root) == pick).or(self.devices.first())
    }

    pub(super) fn cycle_device(&mut self, delta: i32) {
        if self.devices.is_empty() {
            return;
        }
        let i = self.picked_device().and_then(|p| self.devices.iter().position(|v| v.root == p.root)).unwrap_or(0) as i32;
        let next = (i + delta).rem_euclid(self.devices.len() as i32) as usize;
        self.device_pick = Some(self.devices[next].root.clone());
    }

    pub(super) fn picked_catalog(&self) -> Option<PathBuf> {
        Some(device_saves::catalog(&self.picked_device()?.root, &self.selected_game()?.name))
    }

    pub(super) fn insert_launch_config(&mut self) {
        let (Some(game), Some(volume)) = (self.selected_game(), self.picked_device()) else {
            return self.notify(tr!("Plug in a USB stick or SD card first"));
        };
        let path = volume.root.join(disc_file_name(game));
        let (name, device) = (game.name.clone(), volume_name(volume));
        match std::fs::write(&path, disc_manifest(game).to_json()) {
            Ok(()) => self.notify(tr!("Saved {file} on {device}. Plugging it in starts {name}.", file = path.display(), device = device, name = name)),
            Err(e) => self.notify(tr!("Could not save {file}: {error}", file = path.display(), error = e)),
        }
    }

    pub(super) fn create_save_catalog(&mut self) {
        let (Some(catalog), Some(volume)) = (self.picked_catalog(), self.picked_device()) else {
            return self.notify(tr!("Plug in a USB stick or SD card first"));
        };
        let device = volume_name(volume);
        let existed = catalog.is_dir();
        match device_saves::create_catalog(&catalog) {
            Ok(()) if existed => self.notify(tr!("{folder} is already on {device}", folder = catalog.display(), device = device)),
            Ok(()) => self.notify(tr!("Created {folder} on {device}", folder = catalog.display(), device = device)),
            Err(e) => self.notify(tr!("Could not create {folder}: {error}", folder = catalog.display(), error = format!("{e:#}"))),
        }
    }

    pub(super) fn add_device_save_paths(&mut self, folders: bool) {
        let Some(game) = self.selected_game() else { return };
        let start = game.save_dir.clone().filter(|d| d.is_dir()).unwrap_or_else(|| crate::saves::browse_from(game));
        let (title, pick) = if folders {
            (tr!("Choose the folders {name} saves to", name = game.name), Pick::Folders)
        } else {
            (tr!("Choose the files {name} saves to", name = game.name), Pick::Files)
        };
        let dialog = rfd::FileDialog::new().set_title(title).set_directory(start);
        let game_id = game.id.clone();
        let row = if folders { SettingRow::DeviceAddFolders } else { SettingRow::DeviceAddFiles };
        self.ask_paths(dialog, pick, move |app, picked| {
            let Some(i) = app.library.position(&game_id) else { return };
            let paths = &mut app.library.games[i].device_saves.paths;
            let before = paths.len();
            for path in picked {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
            let added = paths.len() - before;
            app.save();
            app.mode = app.settings_at(row);
            app.notify(tr_n!(added, "Tracking {n} more save file or folder", "Tracking {n} more save files or folders"));
        });
    }

    pub(super) fn remove_device_save_path(&mut self, index: usize) {
        let Some(game) = self.library.games.get_mut(self.selected) else { return };
        if index >= game.device_saves.paths.len() {
            return;
        }
        let path = game.device_saves.paths.remove(index);
        let left = game.device_saves.paths.len();
        self.save();
        self.mode = self.settings_at(if left == 0 { SettingRow::DeviceAddFiles } else { SettingRow::DeviceSavePath(index.min(left - 1)) });
        self.notify(tr!("No longer tracking {path}", path = path.display()));
    }

    /// Called when a game YASAFF followed has closed: copies its saves to the device it was
    /// started from, when it and the game are set up for it.
    pub(super) fn after_game_closed(&mut self, game_id: &str, device: Option<Volume>) {
        let Some(volume) = device.filter(|v| v.kind == VolumeKind::Removable) else { return };
        let Some(game) = self.library.position(game_id).map(|i| &self.library.games[i]) else { return };
        if !self.settings.discs.keep_saves || !game.device_saves.keep_on_device {
            return;
        }
        let paths = device_save_paths(game);
        if paths.is_empty() {
            let msg = tr!("No save files are set for {name}, so nothing was copied to {device}", name = game.name, device = volume_name(&volume));
            return self.notify(msg);
        }
        let job = SyncJob { game: game.name.clone(), device_root: volume.root.clone(), paths, recovery: game.device_saves.recovery_count };
        self.start_device_sync(job, volume_name(&volume));
    }

    fn start_device_sync(&mut self, job: SyncJob, device: String) {
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        let work = job.clone();
        let device_name = device.clone();
        let spawned = std::thread::Builder::new().name("yasaff-device-saves".into()).spawn(move || {
            let send = |msg| {
                let _ = tx.send(msg);
                ctx.request_repaint();
            };
            let plugged = crate::physical::removable_volumes().iter().any(|v| v.root == work.device_root);
            let result = if plugged {
                let catalog = device_saves::catalog(&work.device_root, &work.game);
                device_saves::store(&work.game, &work.paths, &catalog, work.recovery, |a| send(SyncMsg::Action(a))).map_err(|e| format!("{e:#}"))
            } else {
                Err(tr!("{device} is not plugged in. Plug it back in and try again.", device = device_name))
            };
            send(SyncMsg::Done(result));
        });
        if let Err(e) = spawned {
            return self.notify(tr!("Could not copy the saves of {name}: {error}", name = job.game, error = e));
        }
        log::info!("copying the saves of {} to {}", job.game, job.device_root.display());
        self.device_sync = Some(DeviceSync { game: job.game.clone(), device, job, rx, action: tr!("Starting").to_owned(), result: None });
        self.focus_guard_until = None;
        self.ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        self.ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    pub(super) fn device_sync_running(&self) -> bool {
        self.device_sync.as_ref().is_some_and(|d| d.result.is_none())
    }

    pub(super) fn poll_device_sync(&mut self) {
        let Some(sync) = &mut self.device_sync else { return };
        loop {
            match sync.rx.try_recv() {
                Ok(SyncMsg::Action(action)) => sync.action = action,
                Ok(SyncMsg::Done(result)) => {
                    match &result {
                        Ok(catalog) => log::info!("saves of {} copied to {}", sync.game, catalog.display()),
                        Err(e) => log::warn!("copying the saves of {} failed: {e}", sync.game),
                    }
                    sync.result = Some(result);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if sync.result.is_none() {
                        sync.result = Some(Err(tr!("the copy stopped unexpectedly").to_owned()));
                    }
                    break;
                }
            }
        }
    }

    /// The saving window takes every button: nothing else happens until it is closed.
    pub(super) fn handle_device_sync(&mut self, action: Action) {
        let Some(sync) = &self.device_sync else { return };
        match (&sync.result, action) {
            (None, _) => {}
            (Some(Err(_)), Action::Confirm) => {
                let (job, device) = (sync.job.clone(), sync.device.clone());
                self.start_device_sync(job, device);
            }
            (Some(_), Action::Confirm | Action::Back) => self.device_sync = None,
            _ => {}
        }
    }

    pub(super) fn device_sync_hints(&self) -> Option<Vec<Hint>> {
        let sync = self.device_sync.as_ref()?;
        Some(match &sync.result {
            None => Vec::new(),
            Some(Ok(_)) => vec![Hint(Action::Confirm, tr!("OK")), Hint(Action::Back, tr!("Close"))],
            Some(Err(_)) => vec![Hint(Action::Confirm, tr!("Try again")), Hint(Action::Back, tr!("Close"))],
        })
    }

    pub(super) fn draw_device_sync(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Some(sync) = &self.device_sync else { return };
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(200));
        let panel = Rect::from_center_size(screen.center(), vec2((900.0 * s).min(screen.width() - 40.0), 420.0 * s));
        painter.rect_filled(panel, (8.0 * s) as u8, Color32::from_rgb(18, 24, 40));
        let left = panel.left() + 40.0 * s;
        let width = panel.width() - 80.0 * s;
        let gray = Color32::from_gray(170);

        let title = match &sync.result {
            None => tr!("Saving files to pointed device: {device}", device = sync.device),
            Some(Ok(_)) => tr!("Saves of {game} are on {device}", game = sync.game, device = sync.device),
            Some(Err(_)) => tr!("Could not save {game} data to {device}", game = sync.game, device = sync.device),
        };
        let title = painter.layout(title, FontId::proportional(28.0 * s), Color32::WHITE, width);
        let mut y = panel.top() + 36.0 * s;
        let title_h = title.size().y;
        painter.galley(pos2(left, y), title, Color32::WHITE);
        y += title_h + 24.0 * s;

        let bar = Rect::from_min_size(pos2(left, y), vec2(width, 22.0 * s));
        let radius = (bar.height() / 2.0) as u8;
        painter.rect_filled(bar, radius, Color32::from_white_alpha(25));
        match &sync.result {
            None => {
                let t = ui.input(|i| i.time) as f32;
                let w = bar.width() * 0.25;
                let x = bar.left() + (bar.width() - w) * (0.5 - 0.5 * (t * 2.0).cos());
                painter.rect_filled(Rect::from_min_size(pos2(x, bar.top()), vec2(w, bar.height())), radius, ACCENT);
                ui.ctx().request_repaint();
            }
            Some(result) => {
                painter.rect_filled(bar, radius, if result.is_ok() { PAD_A } else { PAD_B });
            }
        }
        y = bar.bottom() + 24.0 * s;

        let (status, detail) = match &sync.result {
            None => (
                tr!("Saving {game} data... Please wait", game = sync.game),
                tr!("Current action: {action}", action = sync.action),
            ),
            Some(Ok(catalog)) => (tr!("Saved successfully").to_owned(), catalog.display().to_string()),
            Some(Err(e)) => (tr!("Saving failed").to_owned(), e.clone()),
        };
        painter.text(pos2(left, y), Align2::LEFT_TOP, status, FontId::proportional(22.0 * s), Color32::WHITE);
        y += 40.0 * s;
        let btn = Rect::from_center_size(panel.center_bottom() - vec2(0.0, 50.0 * s), vec2(220.0 * s, 54.0 * s));
        let detail = painter.layout(detail, FontId::proportional(16.0 * s), gray, width);
        let clip = Rect::from_x_y_ranges(panel.x_range(), y..=btn.top() - 12.0 * s);
        painter.with_clip_rect(clip).galley(pos2(left, y), detail, gray);

        let Some(result) = &sync.result else { return };
        let label = if result.is_ok() { tr!("OK") } else { tr!("Try again") };
        painter.rect_filled(btn, (4.0 * s) as u8, Color32::WHITE);
        painter.text(btn.center(), Align2::CENTER_CENTER, label, FontId::proportional(22.0 * s), Color32::BLACK);
        if ui.interact(btn, Id::new("device-sync-ok"), Sense::click()).clicked() {
            self.handle_device_sync(Action::Confirm);
        }
    }
}
