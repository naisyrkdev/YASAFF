
use super::*;

const GUIDE_GUARD: Duration = Duration::from_secs(4);

pub(super) struct RunningGame {
    pub(super) game_id: String,
    pub(super) name: String,
    pub(super) watch: GameWatch,
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
    }

    pub(super) fn close_game(&mut self) {
        self.mode = Mode::Home;
        let Some(game) = &self.game else { return };
        let name = game.name.clone();
        if game.watch.kill() {
            self.notify(tr!("Closed {name}", name = name));
        } else if !game.watch.seen() {
            self.game.take().unwrap().watch.stop();
        } else {
            self.notify(tr!("Could not close {name}", name = name));
        }
    }

    pub(super) fn poll_game(&mut self, ctx: &Context) {
        if !self.game.as_ref().is_some_and(|g| g.watch.ended()) {
            return;
        }
        let game = self.game.take().unwrap();
        if !game.watch.seen() {
            return;
        }
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
    }

    pub(super) fn come_home(&mut self, ctx: &Context) {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        let active = self.front_app_index().or(self.game_index());
        if let Some(i) = active {
            self.focus_guard_until = Some(Instant::now() + GUIDE_GUARD);
            if matches!(self.mode, Mode::Home) {
                self.selected = i;
            }
        }
    }
}
