//! The custom theme's own options: their rows, values, and what Confirm and Left/Right do on them.

use super::*;
use crate::theme_code::{OptionKind, OptionSpec};
use serde_json::Value;

impl App {
    fn theme_option(&self, i: usize) -> Option<&OptionSpec> {
        self.theme_manifest()?.options.get(i)
    }

    fn theme_value(&self, option: &OptionSpec) -> Value {
        option.value(&self.theme().settings.custom.values, &self.fonts)
    }

    pub(in crate::app) fn theme_option_enabled(&self, i: usize) -> bool {
        let Some(option) = self.theme_option(i) else { return false };
        let Some(by) = option.disabled_by.as_deref().and_then(|key| self.theme_manifest()?.option(key)) else { return true };
        self.theme_value(by) != Value::Bool(true)
    }

    pub(in crate::app) fn theme_option_heading(&self, i: usize) -> Option<(&'static str, Option<(String, Color32)>)> {
        let option = self.theme_option(i)?;
        let title = match option.section.as_deref() {
            Some(section) => intern(section),
            None if i == 0 => tr!("Theme options"),
            None => return None,
        };
        let status = option.section_description.as_ref().map(|d| (d.clone(), Color32::from_gray(140)));
        Some((title, status))
    }

    pub(in crate::app) fn theme_option_content(&self, i: usize) -> (String, &'static str, RowValue) {
        let Some(option) = self.theme_option(i) else { return (String::new(), "", RowValue::Summary(String::new())) };
        let sub = intern(&option.description);
        let value = self.theme_value(option);
        let shown = match &option.kind {
            OptionKind::Number(n) => RowValue::Choice(n.label(value.as_f64().unwrap_or(n.default))),
            OptionKind::Toggle { .. } => RowValue::Toggle(value == Value::Bool(true)),
            OptionKind::Color { .. } => RowValue::Color(value.as_str().and_then(theme::parse_hex).unwrap_or(Color32::WHITE)),
            OptionKind::Choice { choices, .. } => {
                let label = choices.iter().find(|c| Some(c.value.as_str()) == value.as_str()).map(|c| c.label.clone());
                RowValue::Choice(label.unwrap_or_default())
            }
            OptionKind::Font => {
                let name = value.as_str().map_or(tr!("Game title font"), fonts::display_name);
                let sub = if self.fonts.is_empty() { tr!("Import fonts in Settings › Fonts to choose one") } else { sub };
                return (option.label.clone(), sub, RowValue::Choice(name.to_owned()));
            }
            OptionKind::Pictures(_) => {
                let n = self.theme().settings.custom.images.len();
                RowValue::Summary(tr_n!(n, "{n} image", "{n} images"))
            }
        };
        (option.label.clone(), sub, shown)
    }

    pub(in crate::app) fn activate_theme_option(&mut self, i: usize) {
        let Some(option) = self.theme_option(i).cloned() else { return };
        if !self.theme_option_enabled(i) {
            return;
        }
        let buffer = match &option.kind {
            OptionKind::Pictures(_) => {
                self.mode = Mode::Settings { cursor: 0, page: Page::ThemePictures, in_rows: true };
                return;
            }
            OptionKind::Number(n) => n.number(self.theme_value(&option).as_f64().unwrap_or(n.default)),
            OptionKind::Color { .. } => self.theme_value(&option).as_str().unwrap_or_default().to_owned(),
            OptionKind::Toggle { .. } | OptionKind::Choice { .. } | OptionKind::Font => return self.adjust_theme_option(i, 1),
        };
        self.mode = Mode::Prompt { kind: PromptKind::ThemeValue(i), buffer };
    }

    pub(in crate::app) fn adjust_theme_option(&mut self, i: usize, delta: i32) {
        let Some(option) = self.theme_option(i).cloned() else { return };
        if !self.theme_option_enabled(i) {
            return;
        }
        let value = self.theme_value(&option);
        let next: Value = match &option.kind {
            OptionKind::Number(n) => n.clamp(value.as_f64().unwrap_or(n.default) + n.step * delta as f64).into(),
            OptionKind::Toggle { .. } => (value != Value::Bool(true)).into(),
            OptionKind::Color { .. } => {
                let color = value.as_str().and_then(theme::parse_hex).unwrap_or(Color32::WHITE);
                let next = match theme::LIGHT_PRESETS.iter().position(|c| *c == color) {
                    Some(_) => cycle(theme::LIGHT_PRESETS, color, delta),
                    None if delta < 0 => *theme::LIGHT_PRESETS.last().unwrap(),
                    None => theme::LIGHT_PRESETS[0],
                };
                theme::to_hex(next).into()
            }
            OptionKind::Choice { choices, .. } => {
                let values: Vec<&str> = choices.iter().map(|c| c.value.as_str()).collect();
                if values.is_empty() {
                    return;
                }
                cycle(&values, value.as_str().unwrap_or(values[0]), delta).into()
            }
            OptionKind::Font => {
                let choices: Vec<Option<String>> = std::iter::once(None).chain(self.fonts.iter().cloned().map(Some)).collect();
                let current = value.as_str().map(str::to_owned);
                let at = choices.iter().position(|f| *f == current).unwrap_or(0) as i32;
                choices[(at + delta).rem_euclid(choices.len() as i32) as usize].clone().map_or(Value::Null, Value::from)
            }
            OptionKind::Pictures(_) => return,
        };
        self.set_theme_value(&option.key, next);
    }

    /// Takes what was typed for a number or color option.
    pub(in crate::app) fn commit_theme_value(&mut self, i: usize, text: &str) {
        let Some(option) = self.theme_option(i).cloned() else { return };
        match &option.kind {
            OptionKind::Number(n) => {
                let typed = text.trim().trim_end_matches(n.unit.as_str()).trim().replace(',', ".");
                match typed.parse::<f64>() {
                    Ok(v) if v.is_finite() => self.set_theme_value(&option.key, n.clamp(v / n.display_scale).into()),
                    _ => self.notify(tr!("\"{text}\" is not a number", text = text)),
                }
            }
            OptionKind::Color { .. } => match theme::parse_hex(text) {
                Some(color) => self.set_theme_value(&option.key, theme::to_hex(color).into()),
                None => self.notify(tr!("\"{text}\" is not a color. Use hex, e.g. #0b2a66", text = text)),
            },
            _ => {}
        }
        self.mode = self.settings_at(SettingRow::ThemeOption(i));
    }

    fn set_theme_value(&mut self, key: &str, value: Value) {
        let values = &mut self.themes[self.theme].settings.custom.values;
        if value.is_null() {
            values.remove(key);
        } else {
            values.insert(key.to_owned(), value);
        }
        self.save_theme();
    }

    /// The prompt's title and note when typing an option's value.
    pub(in crate::app) fn theme_value_prompt(&self, i: usize) -> (&'static str, Option<&'static str>) {
        let Some(option) = self.theme_option(i) else { return ("", None) };
        let note = match &option.kind {
            OptionKind::Number(n) => {
                let unit = if n.unit.is_empty() { String::new() } else { format!(" {}", n.unit) };
                intern(&tr!("From {min} to {max}.", min = n.number(n.min), max = format!("{}{unit}", n.number(n.max))))
            }
            _ => tr!("Hex color: #rrggbb, e.g. #8fc8ff."),
        };
        (intern(&option.label), Some(note))
    }
}
