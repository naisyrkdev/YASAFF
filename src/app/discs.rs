//! Physical discs: reacting to inserted discs, disc files, and burning a game to a disc.

use super::*;

pub(super) struct BurnDialog {
    pub(super) burners: Vec<Burner>,
    pub(super) looked: Instant,
    pub(super) cursor: usize,
    pub(super) armed: bool,
}

pub(super) struct Burning {
    pub(super) game: String,
    pub(super) drive: String,
    pub(super) rx: mpsc::Receiver<BurnMsg>,
    pub(super) progress: burn::Progress,
    pub(super) result: Option<Result<(), String>>,
}

pub(super) enum BurnMsg {
    Progress(burn::Progress),
    Done(Result<(), String>),
}

pub(super) fn disc_from_path(path: &Path) -> anyhow::Result<DiscEvent> {
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

pub(super) fn disc_manifest(game: &Game) -> DiscManifest {
    DiscManifest { id: Some(game.id.clone()), ids: game.external_ids(), name: Some(game.name.clone()) }
}

pub(super) fn disc_file_name(game: &Game) -> String {
    let name: String = game.name.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect();
    format!("{}.yasaff", name.trim())
}

pub(super) fn volume_name(volume: &Volume) -> String {
    let name = match &volume.label {
        Some(label) => label.clone(),
        None => volume.root.display().to_string(),
    };
    match volume.kind {
        VolumeKind::Optical => tr!("disc {name}", name = name),
        VolumeKind::Removable => tr!("drive {name}", name = name),
    }
}

impl App {
    pub(super) fn poll_discs(&mut self, ctx: &Context) {
        while let Ok(event) = self.discs.try_recv() {
            self.on_disc(ctx, event);
        }
    }

    pub(super) fn on_disc(&mut self, ctx: &Context, event: DiscEvent) {
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
        log::info!("disc inserted: {name} ({})", event.volume.root.display());
        if self.burning.as_ref().is_some_and(|b| b.result.is_none())
            || self.discs_quiet_until.is_some_and(|until| Instant::now() < until)
        {
            log::info!("  ignored: just burned");
            return;
        }
        if matches!(self.mode, Mode::Launching(_)) {
            self.notify(tr!("Disc inserted: {name}. Another game is already starting.", name = name));
            return;
        }
        if self.device_sync_running() {
            self.notify(tr!("Disc inserted: {name}. Saves are still being copied. Insert it again afterwards.", name = name));
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
            self.launch_volume = Some(event.volume);
            self.launch_selected();
            return;
        }
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.focus_guard_until = Some(Instant::now() + FOCUS_GUARD);
        let candidates: Vec<String> = candidates.into_iter().filter_map(|i| Some(self.library.games.get(i)?.id.clone())).collect();
        if candidates.is_empty() {
            self.mode = Mode::DiscNotFound { name };
            return;
        }
        self.mode = Mode::DiscChooser { name, candidates, cursor: 0, volume: event.volume };
    }

    pub(super) fn create_disc_file(&mut self) {
        let Some(game) = self.selected_game() else { return };
        let manifest = disc_manifest(game);
        let dialog = rfd::FileDialog::new()
            .set_title(tr!("Save disc file (burn it to the root of a CD/DVD or copy it to a USB stick)"))
            .set_file_name(disc_file_name(game))
            .add_filter(tr!("YASAFF disc file"), &["yasaff"]);
        self.ask_path(dialog, Pick::Save, move |app, path| app.save_disc_file(&manifest, path));
    }

    pub(super) fn save_disc_file(&mut self, manifest: &DiscManifest, path: PathBuf) {
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

    pub(super) fn open_burner(&mut self) {
        if self.selected_game().is_none() && self.burning.is_none() {
            return;
        }
        let burners = burn::burners();
        let cursor = burners.iter().position(|b| b.media.can_burn()).unwrap_or(0);
        self.mode = Mode::Burn(BurnDialog { burners, looked: Instant::now(), cursor, armed: false });
    }

    pub(super) fn start_burn(&mut self) {
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
        log::info!("burning {} on {} ({})", game.name, burner.device.display(), burner.media);
        self.burning = Some(Burning {
            game: game.name.clone(),
            drive: burner.name,
            rx,
            progress: burn::Progress { fraction: None, status: tr!("Starting").into() },
            result: None,
        });
    }

    pub(super) fn poll_burn(&mut self) {
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

    pub(super) fn close_burner(&mut self) {
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
        self.mode = self.settings_at(SettingRow::DeviceBurn);
    }

    pub(super) fn draw_disc_chooser(&mut self, ui: &Ui, screen: Rect, s: f32) {
        let Mode::DiscChooser { name, candidates, cursor, .. } = &self.mode else { return };
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
        for (slot, id) in candidates.iter().enumerate() {
            let Some(game) = self.library.position(id).map(|i| &self.library.games[i]) else { continue };
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

    pub(super) fn draw_disc_not_found(&mut self, ui: &Ui, screen: Rect, s: f32) {
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

    pub(super) fn draw_burn(&mut self, ui: &Ui, screen: Rect, s: f32) {
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
                None => (b.progress.status.clone(), tr!("Don't open the tray. B hides this screen. Burning carries on.").to_owned()),
                Some(Ok(())) => (tr!("Disc burned").to_owned(), tr!("Take it out. Inserting it again starts the game.").to_owned()),
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
            let text = tr!("No CD/DVD burner found. Connect one (USB burners work too). It shows up here.");
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
}

impl App {
    pub(super) fn handle_disc_chooser(&mut self, action: Action) {
        let Mode::DiscChooser { candidates, cursor, .. } = &mut self.mode else { return };
        match action {
            Action::Left | Action::Up => *cursor = cursor.saturating_sub(1),
            Action::Right | Action::Down => *cursor = (*cursor + 1).min(candidates.len() - 1),
            Action::Confirm => {
                let game = candidates.get(*cursor).and_then(|id| self.library.position(id));
                let Mode::DiscChooser { volume, .. } = std::mem::replace(&mut self.mode, Mode::Home) else { return };
                if let Some(i) = game {
                    self.selected = i;
                    self.launch_volume = Some(volume);
                    self.launch_selected();
                }
            }
            Action::Back => self.mode = Mode::Home,
            _ => {}
        }
    }

    pub(super) fn handle_burn(&mut self, action: Action) {
        let Mode::Burn(dialog) = &mut self.mode else { return };
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
}
