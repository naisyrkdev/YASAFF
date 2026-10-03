
use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};
use gilrs::{Axis, Button, EventType, Gilrs};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Left,
    Right,
    Up,
    Down,
    Confirm,
    Back,
    Options,
    Add,
    ToggleFullscreen,
    BringToFront,
    Type(char),
    Erase,
}

impl Action {
    pub fn control(self) -> Control {
        match self {
            Action::Left => Control::Left,
            Action::Right => Control::Right,
            Action::Up => Control::Up,
            Action::Down => Control::Down,
            Action::Confirm => Control::Confirm,
            Action::Back => Control::Back,
            Action::Options => Control::Options,
            Action::Add => Control::Add,
            Action::ToggleFullscreen => Control::Fullscreen,
            Action::BringToFront => Control::Guide,
            Action::Erase => Control::Erase,
            Action::Type(_) => Control::Nothing,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum PadButton {
    South,
    East,
    West,
    North,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    LeftStick,
    RightStick,
    View,
    Menu,
    Guide,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

impl PadButton {
    pub const ALL: [Self; 17] = [
        Self::South,
        Self::East,
        Self::West,
        Self::North,
        Self::LeftBumper,
        Self::RightBumper,
        Self::LeftTrigger,
        Self::RightTrigger,
        Self::LeftStick,
        Self::RightStick,
        Self::View,
        Self::Menu,
        Self::Guide,
        Self::DPadUp,
        Self::DPadDown,
        Self::DPadLeft,
        Self::DPadRight,
    ];

    pub fn glyph(self) -> &'static str {
        match self {
            Self::South => "A",
            Self::East => "B",
            Self::West => "X",
            Self::North => "Y",
            Self::LeftBumper => "LB",
            Self::RightBumper => "RB",
            Self::LeftTrigger => "LT",
            Self::RightTrigger => "RT",
            Self::LeftStick => "LS",
            Self::RightStick => "RS",
            Self::View => "View",
            Self::Menu => "Menu",
            Self::Guide => "Guide",
            Self::DPadUp => "D-pad ↑",
            Self::DPadDown => "D-pad ↓",
            Self::DPadLeft => "D-pad ←",
            Self::DPadRight => "D-pad →",
        }
    }

    fn from_gilrs(button: Button) -> Option<Self> {
        Some(match button {
            Button::South => Self::South,
            Button::East => Self::East,
            Button::West => Self::West,
            Button::North => Self::North,
            Button::LeftTrigger => Self::LeftBumper,
            Button::RightTrigger => Self::RightBumper,
            Button::LeftTrigger2 => Self::LeftTrigger,
            Button::RightTrigger2 => Self::RightTrigger,
            Button::LeftThumb => Self::LeftStick,
            Button::RightThumb => Self::RightStick,
            Button::Select => Self::View,
            Button::Start => Self::Menu,
            Button::Mode => Self::Guide,
            Button::DPadUp => Self::DPadUp,
            Button::DPadDown => Self::DPadDown,
            Button::DPadLeft => Self::DPadLeft,
            Button::DPadRight => Self::DPadRight,
            _ => return None,
        })
    }

    fn to_gilrs(self) -> Button {
        match self {
            Self::South => Button::South,
            Self::East => Button::East,
            Self::West => Button::West,
            Self::North => Button::North,
            Self::LeftBumper => Button::LeftTrigger,
            Self::RightBumper => Button::RightTrigger,
            Self::LeftTrigger => Button::LeftTrigger2,
            Self::RightTrigger => Button::RightTrigger2,
            Self::LeftStick => Button::LeftThumb,
            Self::RightStick => Button::RightThumb,
            Self::View => Button::Select,
            Self::Menu => Button::Start,
            Self::Guide => Button::Mode,
            Self::DPadUp => Button::DPadUp,
            Self::DPadDown => Button::DPadDown,
            Self::DPadLeft => Button::DPadLeft,
            Self::DPadRight => Button::DPadRight,
        }
    }

    pub fn default_control(self) -> Control {
        match self {
            Self::South => Control::Confirm,
            Self::East => Control::Back,
            Self::West => Control::Erase,
            Self::North => Control::Add,
            Self::Menu => Control::Options,
            Self::View => Control::Fullscreen,
            Self::Guide => Control::Guide,
            Self::DPadUp => Control::Up,
            Self::DPadDown => Control::Down,
            Self::DPadLeft => Control::Left,
            Self::DPadRight => Control::Right,
            Self::RightTrigger | Self::RightStick => Control::Click,
            Self::LeftTrigger => Control::RightClick,
            Self::LeftBumper | Self::RightBumper | Self::LeftStick => Control::Nothing,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    Nothing,
    Confirm,
    Back,
    Erase,
    Add,
    Options,
    Fullscreen,
    Guide,
    Up,
    Down,
    Left,
    Right,
    Click,
    RightClick,
}

impl Control {
    pub const ALL: [Self; 14] = [
        Self::Nothing,
        Self::Confirm,
        Self::Back,
        Self::Erase,
        Self::Add,
        Self::Options,
        Self::Fullscreen,
        Self::Guide,
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::Click,
        Self::RightClick,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Nothing => tr!("Nothing"),
            Self::Confirm => tr!("Select / confirm"),
            Self::Back => tr!("Back / cancel"),
            Self::Erase => tr!("Delete"),
            Self::Add => tr!("Add / extra"),
            Self::Options => tr!("Options"),
            Self::Fullscreen => tr!("Toggle fullscreen"),
            Self::Guide => tr!("Bring YASAFF to front"),
            Self::Up => tr!("Up"),
            Self::Down => tr!("Down"),
            Self::Left => tr!("Left"),
            Self::Right => tr!("Right"),
            Self::Click => tr!("Mouse click"),
            Self::RightClick => tr!("Mouse right-click"),
        }
    }

    pub fn action(self) -> Option<Action> {
        Some(match self {
            Self::Confirm => Action::Confirm,
            Self::Back => Action::Back,
            Self::Erase => Action::Erase,
            Self::Add => Action::Add,
            Self::Options => Action::Options,
            Self::Fullscreen => Action::ToggleFullscreen,
            Self::Guide => Action::BringToFront,
            Self::Up => Action::Up,
            Self::Down => Action::Down,
            Self::Left => Action::Left,
            Self::Right => Action::Right,
            Self::Nothing | Self::Click | Self::RightClick => return None,
        })
    }

    fn is_direction(self) -> bool {
        matches!(self, Self::Up | Self::Down | Self::Left | Self::Right)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bindings([Control; PadButton::ALL.len()]);

impl Default for Bindings {
    fn default() -> Self {
        Bindings(PadButton::ALL.map(PadButton::default_control))
    }
}

impl Bindings {
    pub fn get(&self, button: PadButton) -> Control {
        self.0[button as usize]
    }

    pub fn set(&mut self, button: PadButton, control: Control) {
        self.0[button as usize] = control;
    }

    pub fn button_for(&self, control: Control) -> Option<PadButton> {
        PadButton::ALL.into_iter().find(|b| self.get(*b) == control)
    }
}

const STICK_THRESHOLD: f32 = 0.5;
const REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_RATE: Duration = Duration::from_millis(110);
const MOUSE_DEADZONE: f32 = 0.15;
const MOUSE_IDLE: Duration = Duration::from_secs(5);
pub const GLOBAL_MOUSE_BUTTONS: [PadButton; 2] = [PadButton::RightTrigger, PadButton::LeftTrigger];

struct VirtualMouse {
    pos: Option<Pos2>,
    screen: Rect,
    last_tick: Instant,
    last_used: Option<Instant>,
    buttons: Vec<(PointerButton, bool)>,
}

pub struct Input {
    gilrs: Option<Gilrs>,
    held: Option<(Action, Instant)>,
    mouse: VirtualMouse,
    mouse_enabled: bool,
    global_mouse: bool,
    bindings: Bindings,
}

impl Input {
    pub fn new() -> Self {
        let gilrs = match Gilrs::new() {
            Ok(g) => Some(g),
            Err(e) => {
                eprintln!("gamepad support unavailable: {e}");
                None
            }
        };
        let mouse = VirtualMouse { pos: None, screen: Rect::NOTHING, last_tick: Instant::now(), last_used: None, buttons: Vec::new() };
        Self { gilrs, held: None, mouse, mouse_enabled: false, global_mouse: false, bindings: Bindings::default() }
    }

    pub fn set_bindings(&mut self, bindings: Bindings) {
        self.bindings = bindings;
        self.held = None;
    }

    pub fn bindings(&self) -> &Bindings {
        &self.bindings
    }

    pub fn set_virtual_mouse(&mut self, enabled: bool) {
        self.mouse_enabled = enabled;
        if !self.mouse_active() {
            self.mouse.last_used = None;
            self.mouse.buttons.clear();
        }
    }

    pub fn set_global_mouse(&mut self, running: bool) {
        if running == self.global_mouse {
            return;
        }
        self.global_mouse = running;
        self.held = None;
        self.set_virtual_mouse(self.mouse_enabled);
    }

    fn mouse_active(&self) -> bool {
        self.mouse_enabled && !self.global_mouse
    }

    pub fn virtual_pointer(&self) -> Option<Pos2> {
        let recent = self.mouse.last_used.is_some_and(|t| t.elapsed() < MOUSE_IDLE);
        self.mouse.pos.filter(|_| self.mouse_active() && recent)
    }

    pub fn feed_virtual_mouse(&mut self, raw: &mut RawInput, speed: f32) {
        let active = self.mouse_active();
        let m = &mut self.mouse;
        let now = Instant::now();
        let dt = (now - m.last_tick).as_secs_f32().min(0.05);
        m.last_tick = now;
        if let Some(rect) = raw.screen_rect {
            m.screen = rect;
        }
        for event in &raw.events {
            if let Event::PointerMoved(p) = event {
                m.pos = Some(*p);
                m.last_used = None;
            }
        }
        let Some(gilrs) = &self.gilrs else { return };
        if !active || !raw.focused || !m.screen.is_positive() {
            m.buttons.clear();
            return;
        }
        let stick = gilrs
            .gamepads()
            .map(|(_, pad)| egui::vec2(pad.value(Axis::RightStickX), -pad.value(Axis::RightStickY)))
            .find(|v| v.length() > MOUSE_DEADZONE);
        if let Some(v) = stick {
            let tilt = ((v.length() - MOUSE_DEADZONE) / (1.0 - MOUSE_DEADZONE)).min(1.0);
            let step = v.normalized() * tilt * tilt * speed * (m.screen.height() / 1080.0) * dt;
            let from = m.pos.unwrap_or(m.screen.center());
            let to = m.screen.clamp(from + step);
            m.pos = Some(to);
            m.last_used = Some(now);
            raw.events.push(Event::PointerMoved(to));
        }
        if m.buttons.is_empty() {
            return;
        }
        let pos = *m.pos.get_or_insert(m.screen.center());
        m.last_used = Some(now);
        for (button, pressed) in m.buttons.drain(..) {
            raw.events.push(Event::PointerButton { pos, button, pressed, modifiers: Modifiers::NONE });
        }
    }

    pub fn poll_hidden(&mut self) -> Vec<Action> {
        self.held = None;
        let global_mouse = self.global_mouse;
        let Some(gilrs) = &mut self.gilrs else { return Vec::new() };
        let mut actions = Vec::new();
        while let Some(ev) = gilrs.next_event() {
            if let EventType::ButtonPressed(b, _) = ev.event
                && PadButton::from_gilrs(b).is_some_and(|b| !left_to_global_mouse(global_mouse, b) && self.bindings.get(b) == Control::Guide)
            {
                actions.push(Action::BringToFront);
            }
        }
        actions
    }

    pub fn poll(&mut self, ctx: &Context, capture_text: bool) -> Vec<Action> {
        let focused = ctx.input(|i| i.focused);
        let (global_mouse, mouse_active) = (self.global_mouse, self.mouse_active());
        let mut actions = Vec::new();

        if let Some(gilrs) = &mut self.gilrs {
            while let Some(ev) = gilrs.next_event() {
                let (button, pressed) = match ev.event {
                    EventType::ButtonPressed(b, _) => (b, true),
                    EventType::ButtonReleased(b, _) => (b, false),
                    _ => continue,
                };
                let Some(button) = PadButton::from_gilrs(button).filter(|b| !left_to_global_mouse(global_mouse, *b)) else { continue };
                let control = self.bindings.get(button);
                if mouse_active && focused {
                    let pointer = match control {
                        Control::Click => Some(PointerButton::Primary),
                        Control::RightClick => Some(PointerButton::Secondary),
                        _ => None,
                    };
                    if let Some(pointer) = pointer {
                        self.mouse.buttons.push((pointer, pressed));
                        continue;
                    }
                }
                if !pressed || control.is_direction() {
                    continue;
                }
                let Some(action) = control.action() else { continue };
                if focused || action == Action::BringToFront {
                    actions.push(action);
                }
            }
            let dir = if focused { held_direction(gilrs, &self.bindings, self.global_mouse) } else { None };
            self.update_repeat(dir, &mut actions);
        }

        if focused && !ctx.text_edit_focused() {
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Text(text) = event
                        && capture_text
                    {
                        actions.extend(text.chars().filter(|c| !c.is_control()).map(Action::Type));
                        continue;
                    }
                    let egui::Event::Key { key, pressed: true, .. } = event else { continue };
                    let action = match key {
                        Key::Space if capture_text => continue,
                        Key::Backspace if capture_text => Action::Erase,
                        Key::ArrowLeft => Action::Left,
                        Key::ArrowRight => Action::Right,
                        Key::ArrowUp => Action::Up,
                        Key::ArrowDown => Action::Down,
                        Key::Enter | Key::Space => Action::Confirm,
                        Key::Escape | Key::Backspace => Action::Back,
                        Key::Tab => Action::Options,
                        Key::Insert => Action::Add,
                        Key::F11 => Action::ToggleFullscreen,
                        _ => continue,
                    };
                    actions.push(action);
                }
            });
        }
        actions
    }

    fn update_repeat(&mut self, dir: Option<Action>, actions: &mut Vec<Action>) {
        let now = Instant::now();
        match (dir, self.held) {
            (None, _) => self.held = None,
            (Some(d), Some((prev, next))) if d == prev => {
                if now >= next {
                    actions.push(d);
                    self.held = Some((d, now + REPEAT_RATE));
                }
            }
            (Some(d), _) => {
                actions.push(d);
                self.held = Some((d, now + REPEAT_DELAY));
            }
        }
    }

    pub fn has_gamepad(&self) -> bool {
        self.gilrs.as_ref().is_some_and(|g| g.gamepads().next().is_some())
    }
}

fn left_to_global_mouse(global_mouse: bool, button: PadButton) -> bool {
    global_mouse && GLOBAL_MOUSE_BUTTONS.contains(&button)
}

fn held_direction(gilrs: &Gilrs, bindings: &Bindings, global_mouse: bool) -> Option<Action> {
    for (_, pad) in gilrs.gamepads() {
        let x = pad.value(Axis::LeftStickX);
        let y = pad.value(Axis::LeftStickY);
        let held = |control: Control| {
            PadButton::ALL
                .into_iter()
                .filter(|b| !left_to_global_mouse(global_mouse, *b))
                .any(|b| bindings.get(b) == control && pad.is_pressed(b.to_gilrs()))
        };
        let dir = if held(Control::Left) || x < -STICK_THRESHOLD {
            Some(Action::Left)
        } else if held(Control::Right) || x > STICK_THRESHOLD {
            Some(Action::Right)
        } else if held(Control::Up) || y > STICK_THRESHOLD {
            Some(Action::Up)
        } else if held(Control::Down) || y < -STICK_THRESHOLD {
            Some(Action::Down)
        } else {
            None
        };
        if dir.is_some() {
            return dir;
        }
    }
    None
}
