//! Game save folders: finding them, opening them and backing them up.

use super::*;

impl App {
    pub(super) fn pick_save_dir(&mut self) {
        let Some(game) = self.selected_game() else { return };
        let start = game.save_dir.clone().filter(|d| d.is_dir()).unwrap_or_else(|| crate::saves::browse_from(game));
        let dialog = rfd::FileDialog::new().set_title(tr!("Choose the folder {name} keeps its saves in", name = game.name)).set_directory(start);
        let game_id = game.id.clone();
        self.ask_path(dialog, Pick::Folder, move |app, dir| {
            let Some(i) = app.library.position(&game_id) else { return };
            let name = app.library.games[i].name.clone();
            app.library.games[i].save_dir = Some(dir.clone());
            app.save();
            app.mode = app.settings_at(SettingRow::DeviceSetSaveFolder);
            app.notify(tr!("Saves of {name}: {folder}", name = name, folder = dir.display()));
        });
    }

    pub(super) fn open_save_dir(&mut self) {
        let Some(dir) = self.selected_game().and_then(|g| g.save_dir.clone()) else { return };
        if let Err(e) = open::that_detached(&dir) {
            self.notify(tr!("Could not open {folder}: {error}", folder = dir.display(), error = e));
        }
    }

    pub(super) fn backup_saves(&mut self) {
        let Some(game) = self.selected_game() else { return };
        let Some(save_dir) = game.save_dir.clone() else { return };
        if self.backup.is_some() {
            return self.notify(tr!("A backup is already being made"));
        }
        let name = game.name.clone();
        let mut dialog = rfd::FileDialog::new().set_title(tr!("Choose where to save the backup of {name}", name = name));
        let on_device = self.picked_device().map(|v| self.picked_catalog().filter(|c| c.is_dir()).unwrap_or_else(|| v.root.clone()));
        if let Some(dir) = on_device.or_else(|| self.settings.backup_dir.clone()).filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        self.ask_path(dialog, Pick::Folder, move |app, dest| app.start_backup(name, save_dir, dest));
    }

    pub(super) fn start_backup(&mut self, name: String, save_dir: PathBuf, dest: PathBuf) {
        if self.backup.is_some() {
            return self.notify(tr!("A backup is already being made"));
        }
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

    pub(super) fn poll_backup(&mut self) {
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

    pub(super) fn find_missing_save_dirs(&mut self) {
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

    pub(super) fn poll_save_dirs(&mut self) {
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
}
