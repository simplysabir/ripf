//! Parsing for the `[keys]` table in config.toml.
//!
//! Only actions with a single default binding are rebindable. Movement
//! (arrows *and* ctrl-j/k), enter and esc stay fixed: they have two bindings
//! each or are load-bearing enough that letting someone unbind them is a
//! footgun, not a feature.

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl Chord {
    const fn new(code: KeyCode, mods: KeyModifiers) -> Self {
        Self { code, mods }
    }

    pub fn matches(&self, code: KeyCode, mods: KeyModifiers) -> bool {
        // Terminals set extra bits (KEYPAD, NONE variants) that a plain `==`
        // would reject, so compare only the modifiers we bind on.
        let relevant = mods & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        self.code == code && relevant == self.mods
    }
}

/// Parses `"ctrl-f"`, `"tab"`, `"alt-shift-p"`. Case-insensitive.
pub fn parse(spec: &str) -> Result<Chord, String> {
    let lower = spec.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return Err("empty key binding".to_string());
    }

    let mut parts: Vec<&str> = lower.split('-').collect();
    // Last segment is the key; everything before it is a modifier.
    let key = parts.pop().expect("split always yields one element");

    let mut mods = KeyModifiers::NONE;
    for part in parts {
        match part {
            "ctrl" => mods |= KeyModifiers::CONTROL,
            "alt" => mods |= KeyModifiers::ALT,
            "shift" => mods |= KeyModifiers::SHIFT,
            other => return Err(format!("unknown modifier `{other}` in `{spec}`")),
        }
    }

    let code = match key {
        "tab" => KeyCode::Tab,
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "backspace" => KeyCode::Backspace,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        k if k.chars().count() == 1 => KeyCode::Char(k.chars().next().expect("length checked")),
        other => return Err(format!("unknown key `{other}` in `{spec}`")),
    };

    Ok(Chord { code, mods })
}

#[derive(Debug, Clone, Copy)]
pub struct Keymap {
    pub toggle_mode: Chord,
    pub toggle_preview: Chord,
    pub refresh: Chord,
    pub mark: Chord,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            toggle_mode: Chord::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
            toggle_preview: Chord::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
            refresh: Chord::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            mark: Chord::new(KeyCode::Tab, KeyModifiers::NONE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_modifier_and_a_char() {
        assert_eq!(
            parse("ctrl-f").unwrap(),
            Chord::new(KeyCode::Char('f'), KeyModifiers::CONTROL)
        );
    }

    #[test]
    fn parses_a_bare_named_key() {
        assert_eq!(
            parse("tab").unwrap(),
            Chord::new(KeyCode::Tab, KeyModifiers::NONE)
        );
    }

    #[test]
    fn parses_stacked_modifiers_case_insensitively() {
        let got = parse("Alt-Shift-P").unwrap();
        assert_eq!(
            got,
            Chord::new(KeyCode::Char('p'), KeyModifiers::ALT | KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn rejects_unknown_modifiers_and_keys() {
        assert!(parse("hyper-f").is_err());
        assert!(parse("ctrl-nonsense").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn matching_ignores_irrelevant_modifier_bits() {
        let chord = Chord::new(KeyCode::Tab, KeyModifiers::NONE);
        assert!(chord.matches(KeyCode::Tab, KeyModifiers::NONE));
        assert!(!chord.matches(KeyCode::Tab, KeyModifiers::CONTROL));
    }
}
