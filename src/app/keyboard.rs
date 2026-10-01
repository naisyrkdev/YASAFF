
use super::*;

const COLS: usize = 10;
const DIGITS: &str = "1234567890";
const LETTERS: [&str; 3] = ["qwertyuiop", "asdfghjkl'", "zxcvbnm,.-"];
const SYMBOLS: [&str; 3] = ["!@#$%^&*()", "-_=+[]{};:", "\"/\\?|`~<>&"];

const SPECIAL: [(Special, usize); 4] = [(Special::Shift, 2), (Special::Symbols, 2), (Special::Space, 4), (Special::Erase, 2)];
const SPECIAL_ROW: usize = 4;

#[derive(Clone, Copy, PartialEq)]
enum Special {
    Shift,
    Symbols,
    Space,
    Erase,
}

#[derive(Clone, Copy, PartialEq)]
enum Key {
    Char(char),
    Special(Special),
}

#[derive(Default)]
pub(super) struct Keyboard {
    row: usize,
    col: usize,
    shift: bool,
    symbols: bool,
    typed: bool,
}

impl Keyboard {
    fn char_at(&self, row: usize, col: usize) -> char {
        let line = match row {
            0 => DIGITS,
            r if self.symbols => SYMBOLS[r - 1],
            r => LETTERS[r - 1],
        };
        let c = line.chars().nth(col).unwrap_or(' ');
        if self.shift { c.to_ascii_uppercase() } else { c }
    }

    fn special_at(col: usize) -> (Special, usize) {
        let mut start = 0;
        for (key, span) in SPECIAL {
            if col < start + span {
                return (key, start);
            }
            start += span;
        }
        (SPECIAL[SPECIAL.len() - 1].0, COLS - SPECIAL[SPECIAL.len() - 1].1)
    }

    fn key_at(&self, row: usize, col: usize) -> Key {
        if row == SPECIAL_ROW { Key::Special(Self::special_at(col).0) } else { Key::Char(self.char_at(row, col)) }
    }

    pub(super) fn navigate(&mut self, action: Action) {
        match action {
            Action::Up => self.row = (self.row + SPECIAL_ROW) % (SPECIAL_ROW + 1),
            Action::Down => self.row = (self.row + 1) % (SPECIAL_ROW + 1),
            Action::Left if self.row == SPECIAL_ROW => {
                let (_, start) = Self::special_at(self.col);
                self.col = Self::special_at((start + COLS - 1) % COLS).1;
            }
            Action::Right if self.row == SPECIAL_ROW => {
                let (key, start) = Self::special_at(self.col);
                let span = SPECIAL.iter().find(|(k, _)| *k == key).map_or(1, |(_, s)| *s);
                self.col = (start + span) % COLS;
            }
            Action::Left => self.col = (self.col + COLS - 1) % COLS,
            Action::Right => self.col = (self.col + 1) % COLS,
            _ => {}
        }
        if self.row == SPECIAL_ROW {
            self.col = Self::special_at(self.col).1;
        }
    }

    pub(super) fn press_selected(&mut self, buffer: &mut String) {
        self.press(self.key_at(self.row, self.col), buffer);
    }

    pub(super) fn erase(&mut self, buffer: &mut String) {
        self.typed |= buffer.pop().is_some();
    }

    pub(super) fn take_typed(&mut self) -> bool {
        std::mem::take(&mut self.typed)
    }

    fn press(&mut self, key: Key, buffer: &mut String) {
        if let Key::Char(_) | Key::Special(Special::Space) = key {
            self.typed = true;
        }
        match key {
            Key::Char(c) => {
                buffer.push(c);
                self.shift = false;
            }
            Key::Special(Special::Space) => buffer.push(' '),
            Key::Special(Special::Erase) => self.erase(buffer),
            Key::Special(Special::Shift) => self.shift = !self.shift,
            Key::Special(Special::Symbols) => self.symbols = !self.symbols,
        }
    }

    pub(super) fn toggle_shift(&mut self) {
        self.shift = !self.shift;
    }

    pub(super) fn show(&mut self, ui: &mut Ui, buffer: &mut String) {
        let gap = 4.0;
        let unit = (ui.available_width() - gap * (COLS - 1) as f32) / COLS as f32;
        let size = |span: usize| vec2(unit * span as f32 + gap * (span - 1) as f32, unit * 0.8);
        let mut pressed = None;
        ui.spacing_mut().item_spacing = vec2(gap, gap);
        for row in 0..=SPECIAL_ROW {
            ui.horizontal(|ui| {
                let keys: Vec<(Key, usize, usize)> = if row == SPECIAL_ROW {
                    let mut start = 0;
                    SPECIAL
                        .iter()
                        .map(|&(key, span)| {
                            start += span;
                            (Key::Special(key), start - span, span)
                        })
                        .collect()
                } else {
                    (0..COLS).map(|col| (self.key_at(row, col), col, 1)).collect()
                };
                for (key, col, span) in keys {
                    let label = match key {
                        Key::Char(c) => c.to_string(),
                        Key::Special(Special::Shift) => tr!("Shift").into(),
                        Key::Special(Special::Symbols) => (if self.symbols { "abc" } else { "#+=" }).into(),
                        Key::Special(Special::Space) => tr!("Space").into(),
                        Key::Special(Special::Erase) => tr!("Delete").into(),
                    };
                    let on = match key {
                        Key::Special(Special::Shift) => self.shift,
                        Key::Special(Special::Symbols) => self.symbols,
                        _ => false,
                    };
                    let here = row == self.row && col == self.col;
                    let mut button = egui::Button::new(egui::RichText::new(label).size(unit * 0.36)).min_size(size(span)).selected(on);
                    if here {
                        button = button.stroke(Stroke::new(2.5, ui.visuals().selection.stroke.color));
                    }
                    if ui.add(button).clicked() {
                        (self.row, self.col) = (row, col);
                        pressed = Some(key);
                    }
                }
            });
        }
        if let Some(key) = pressed {
            self.press(key, buffer);
        }
    }
}
