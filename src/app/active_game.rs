
use super::*;

const GUIDE_GUARD: Duration = Duration::from_secs(4);

pub(super) struct RunningGame {
    pub(super) game_id: String,
    pub(super) name: String,
    pub(super) watch: GameWatch,
    /// The USB stick, SD card or disc the game was started from.
    pub(super) device: Option<Volume>,
}

impl App {
    pub(super) fn set_running_game(&mut self, game: RunningGame) {
        if let Some(old) = self.game.replace(game) {
            old.watch.stop();
        }
    }

    pub(super) fn game_index(&self) -> Option<usize> {
        self.library.position(&self.game.as_ref()?.game_id)
    }

    pub(super) fn is_active(&self, i: usize) -> bool {
        self.app_running_at(i) || self.game_index() == Some(i)
    }

    pub(super) fn switch_to_game(&mut self) {
        self.mode = Mode::Home;
        self.focus_guard_until = None;
        self.ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        self.set_taskbar_hidden(false);
    }

    pub(super) fn close_game(&mut self) {
        self.mode = Mode::Home;
        let Some(game) = &self.game else { return };
        let name = game.name.clone();
        if game.watch.kill() {
            self.notify(tr!("Closed {name}", name = name));
        } else if !game.watch.tracked()
            && let Some(game) = self.game.take()
        {
            game.watch.stop();
        } else {
            self.notify(tr!("Could not close {name}", name = name));
        }
    }

    pub(super) fn poll_game(&mut self, ctx: &Context) {
        if !self.game.as_ref().is_some_and(|g| g.watch.ended()) {
            return;
        }
        let Some(game) = self.game.take() else { return };
        if !game.watch.tracked() {
            return;
        }
        self.after_game_closed(&game.game_id, game.device);
        if matches!(self.mode, Mode::Launching(_)) {
            self.mode = Mode::Home;
        }
        if matches!(self.mode, Mode::Home)
            && let Some(i) = self.library.position(&game.game_id)
        {
            self.selected = i;
        }
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.sync_taskbar();
    }

    pub(super) fn come_home(&mut self, ctx: &Context) {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.sync_taskbar();
        let active = self.front_app_index().or(self.game_index());
        if let Some(i) = active {
            self.focus_guard_until = Some(Instant::now() + GUIDE_GUARD);
            if matches!(self.mode, Mode::Home) {
                self.selected = i;
            }
        }
    }
}

pub(super) struct Launching {
    pub(super) name: String,
    pub(super) font: egui::FontFamily,
    pub(super) cover: Option<PathBuf>,
    pub(super) via: Option<Store>,
    pub(super) store_app: bool,
    pub(super) started: Instant,
    pub(super) watch: GameWatch,
}

impl Drop for Launching {
    fn drop(&mut self) {
        if self.store_app {
            self.watch.stop();
        }
    }
}

impl App {
    pub(super) fn activate_selected(&mut self) {
        match self.special(self.selected) {
            None => self.launch_selected(),
            Some(Special::Store) => self.open_store(),
            Some(Special::Add) => self.mode = Mode::AddMenu { cursor: 0 },
            Some(Special::Settings) => self.mode = Mode::Settings { cursor: 0, page: Page::App, in_rows: false },
        }
    }

    pub(super) fn launch_selected(&mut self) {
        let device = self.launch_volume.take();
        let Some(game) = self.selected_game() else { return };
        if game.app_id.is_some() {
            return self.launch_app();
        }
        if self.game.as_ref().is_some_and(|g| g.game_id == game.id) {
            return self.switch_to_game();
        }
        let (name, cover, font, game_id) = (game.name.clone(), game.asset(AssetKind::Cover).map(Path::to_path_buf), self.game_font(game), game.id.clone());
        match launcher::launch(game, &self.settings) {
            Ok(launcher::Launched { via, pid }) => {
                self.selected = self.library.mark_played(self.selected, self.settings.order.moves_played());
                self.save();
                let watch = GameWatch::start(self.ctx.clone(), pid);
                self.set_running_game(RunningGame { game_id, name: name.clone(), watch: watch.clone(), device });
                self.focus_guard_until = None;
                self.mode = Mode::Launching(Launching { name, font, cover, via, store_app: false, started: Instant::now(), watch });
            }
            Err(e) => self.notify(tr!("Could not start {name}: {error}", name = name, error = format!("{e:#}"))),
        }
    }

    pub(super) fn update_launching(&mut self, ctx: &Context) {
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
                self.set_taskbar_hidden(false);
            }
        } else if elapsed >= LAUNCH_TIMEOUT {
            let msg = tr!("{name} hasn't opened a window yet. It may still be starting.", name = l.name);
            self.mode = Mode::Home;
            self.notify(msg);
        }
    }

    pub(super) fn guard_focus(&mut self, ctx: &Context) {
        let Some(until) = self.focus_guard_until else { return };
        if Instant::now() >= until || matches!(self.mode, Mode::Launching(_)) {
            self.focus_guard_until = None;
        } else if !ctx.input(|i| i.focused) {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
    }

    pub(super) fn draw_launching(&self, ui: &Ui, screen: Rect, s: f32) {
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
            painter.text(pos2(x, y + 30.0 * s), Align2::CENTER_TOP, tr!("Taking a while? Press B to hide this screen. The game keeps starting."), FontId::proportional(16.0 * s), Color32::from_gray(120));
        }
    }
}
