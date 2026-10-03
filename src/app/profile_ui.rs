//! Profiles (switch, export, import, reset), the restart they need, and first-run setup.

use super::*;

pub(super) struct InitialSetup {
    pub(super) rx: mpsc::Receiver<initial::Step>,
    pub(super) total: usize,
    pub(super) done: usize,
}

pub(super) enum ProfileJob {
    Exported(Result<PathBuf, String>),
    Imported(Result<profiles::Imported, String>),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Start {
    Import,
    Blank,
    Demo,
}

pub(super) const WELCOME: [(Start, &str, &str); 3] = [
    (Start::Import, tr_key!("Import games"), tr_key!("Pick from the Steam, Heroic and shortcut games installed on this computer")),
    (Start::Blank, tr_key!("Start blank"), tr_key!("An empty library; add games any time with Add game")),
    (Start::Demo, tr_key!("Start blank (with mock games)"), tr_key!("All customization images included")),
];

impl App {
    /// What would be cut short by a restart or by deleting the profile, if anything.
    pub(super) fn busy_with(&self) -> Option<&'static str> {
        if self.burning.as_ref().is_some_and(|b| b.result.is_none()) {
            Some(tr!("a disc is being burned"))
        } else if self.backup.is_some() {
            Some(tr!("a save backup is being made"))
        } else if self.profile_job.is_some() {
            Some(tr!("a profile is being exported or imported"))
        } else if self.setup.is_some() {
            Some(tr!("the bundled games are being installed"))
        } else if self.store_busy() {
            Some(tr!("something is downloading from the store"))
        } else {
            None
        }
    }

    /// False (after telling the user why) when a background job must finish first.
    pub(super) fn idle_or_notify(&mut self) -> bool {
        match self.busy_with() {
            Some(what) => {
                self.notify(tr!("Wait until {what} finishes, then try again", what = what));
                false
            }
            None => true,
        }
    }

    /// Closes the window so `main` can start YASAFF again after everything shut down cleanly
    /// (addons stopped, files closed), instead of exiting the process from inside a frame.
    pub(super) fn restart(&mut self) {
        self.restarting = true;
        profiles::request_restart();
        self.ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    pub(super) fn export_profile(&mut self) {
        if self.profile_job.is_some() {
            return self.notify(tr!("A profile export or import is already running"));
        }
        let name = profiles::active();
        let dialog = rfd::FileDialog::new().set_title(tr!("Export profile {name}", name = name)).add_filter("ZIP", &["zip"]).set_file_name(format!("{name}.zip"));
        self.ask_path(dialog, Pick::Save, move |app, mut dest| {
            if app.profile_job.is_some() {
                return app.notify(tr!("A profile export or import is already running"));
            }
            if dest.extension().is_none_or(|e| !e.eq_ignore_ascii_case("zip")) {
                dest.as_mut_os_string().push(".zip");
            }
            app.start_profile_job(move || ProfileJob::Exported(profiles::export(&dest).map(|()| dest).map_err(|e| format!("{e:#}"))));
            app.notify(tr!("Exporting profile {name}…", name = name));
        });
    }

    pub(super) fn import_profile(&mut self) {
        if self.profile_job.is_some() {
            return self.notify(tr!("A profile export or import is already running"));
        }
        let dialog = rfd::FileDialog::new().set_title(tr!("Select an exported profile")).add_filter("ZIP", &["zip"]);
        self.ask_path(dialog, Pick::File, |app, path| {
            if app.profile_job.is_some() {
                return app.notify(tr!("A profile export or import is already running"));
            }
            app.start_profile_job(move || ProfileJob::Imported(profiles::import(&path).map_err(|e| format!("{e:#}"))));
            app.notify(tr!("Importing profile…"));
        });
    }

    pub(super) fn start_profile_job(&mut self, job: impl FnOnce() -> ProfileJob + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            if tx.send(job()).is_ok() {
                ctx.request_repaint();
            }
        });
        self.profile_job = Some(rx);
    }

    pub(super) fn poll_profile_job(&mut self) {
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

    pub(super) fn poll_setup(&mut self) {
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
                    log::warn!("initial games: {e}");
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

    pub(super) fn draw_setup(&self, painter: &Painter, screen: Rect, s: f32) {
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

    pub(super) fn reset_profiles(&mut self, everything: bool) {
        if !self.idle_or_notify() {
            return;
        }
        let result = if everything {
            if self.autostart {
                let _ = autostart::set(false);
            }
            profiles::reset_all()
        } else {
            profiles::reset_current()
        };
        match result {
            Ok(()) => self.restart(),
            Err(e) => self.notify(tr!("Could not reset: {error}", error = format!("{e:#}"))),
        }
    }

    pub(super) fn draw_profile_imported(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::ProfileImported { name, missing, switch } = &self.mode else { return };
        const SHOWN: usize = 12;
        let painter = ui.painter();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(160));
        let lines = missing.len().min(SHOWN + 1);
        let height = (290.0 + if missing.is_empty() { 0.0 } else { 40.0 + 26.0 * lines as f32 }) * s;
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
        // An imported library carries launch commands that run as the user; say so before switching.
        let warning = tr!("Its games run the launch commands saved in it. Only use profiles from people you trust.");
        painter.text(dialog.center_bottom() + vec2(0.0, -120.0 * s), Align2::CENTER_CENTER, warning, FontId::proportional(16.0 * s), PAD_Y);
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

    pub(super) fn draw_welcome(&mut self, ui: &Ui, screen: Rect, s: f32) {
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

    pub(super) fn start_with(&mut self, start: Start) {
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
}

impl App {
    pub(super) fn handle_welcome(&mut self, action: Action) {
        let Mode::Welcome { cursor } = &mut self.mode else { return };
        match action {
            Action::Up => *cursor = cursor.saturating_sub(1),
            Action::Down => *cursor = (*cursor + 1).min(WELCOME.len() - 1),
            Action::Confirm => {
                let start = WELCOME[*cursor].0;
                self.start_with(start);
            }
            _ => {}
        }
    }

    pub(super) fn handle_profile_imported(&mut self, action: Action) {
        let Mode::ProfileImported { name, switch, .. } = &mut self.mode else { return };
        match action {
            Action::Left | Action::Right | Action::Up | Action::Down => *switch = !*switch,
            Action::Confirm if *switch => {
                let name = name.clone();
                if !self.idle_or_notify() {
                    return;
                }
                match profiles::switch(&name) {
                    Ok(()) => self.restart(),
                    Err(e) => self.notify(tr!("Could not switch profile: {error}", error = format!("{e:#}"))),
                }
            }
            Action::Confirm | Action::Back => self.mode = self.settings_at(SettingRow::ProfileImport),
            _ => {}
        }
    }
}
