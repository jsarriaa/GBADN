//! Which keyboard keys and gamepad buttons press which GBA button.

use eframe::egui::Key;
use gilrs::{Axis, Button, Gilrs};

use crate::keys;

/// How far a stick must lean to count as the D-pad.
const STICK_THRESHOLD: f32 = 0.5;

/// Keys the console keeps for itself, so a game button can't take them.
pub const RESERVED_KEYS: [Key; 3] = [Key::F5, Key::F9, Key::Escape];

/// Every GBA button with its name and default key, in display order.
pub const BUTTONS: [(u32, &str, Key); 10] = [
    (keys::UP, "Up", Key::ArrowUp),
    (keys::DOWN, "Down", Key::ArrowDown),
    (keys::LEFT, "Left", Key::ArrowLeft),
    (keys::RIGHT, "Right", Key::ArrowRight),
    (keys::A, "A", Key::Z),
    (keys::B, "B", Key::X),
    (keys::L, "L", Key::A),
    (keys::R, "R", Key::S),
    (keys::START, "Start", Key::Enter),
    (keys::SELECT, "Select", Key::Backspace),
];

/// The key assigned to each GBA button, in the order of [`BUTTONS`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBindings([Key; BUTTONS.len()]);

impl Default for KeyBindings {
    fn default() -> Self {
        Self(BUTTONS.map(|(_, _, key)| key))
    }
}

impl KeyBindings {
    /// The key assigned to a button.
    pub fn key(&self, button: u32) -> Key {
        self.0[index_of(button)]
    }

    /// Assigns a key. If another button had it, the two swap, so no button is left without one.
    pub fn assign(&mut self, button: u32, key: Key) {
        let index = index_of(button);
        if let Some(other) = self.0.iter().position(|bound| *bound == key) {
            self.0[other] = self.0[index];
        }
        self.0[index] = key;
    }

    /// The buttons held, given which keys are down.
    pub fn pressed(&self, is_down: impl Fn(Key) -> bool) -> u32 {
        BUTTONS
            .iter()
            .zip(self.0)
            .filter(|(_, key)| is_down(*key))
            .fold(0, |mask, ((button, _, _), _)| mask | button)
    }

    /// Writes the bindings as text for the settings file.
    pub fn to_text(&self) -> String {
        BUTTONS
            .iter()
            .zip(self.0)
            .map(|((_, name, _), key)| format!("{name}={}", key.name()))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Reads bindings written by [`KeyBindings::to_text`]. Anything unusable keeps its default.
    pub fn from_text(text: &str) -> Self {
        let mut bindings = Self::default();
        for entry in text.split(';') {
            let Some((name, key)) = entry.split_once('=') else {
                continue;
            };
            let button = BUTTONS.iter().find(|(_, n, _)| *n == name);
            if let (Some((button, _, _)), Some(key)) = (button, Key::from_name(key))
                && !RESERVED_KEYS.contains(&key)
            {
                bindings.assign(*button, key);
            }
        }
        bindings
    }
}

fn index_of(button: u32) -> usize {
    BUTTONS
        .iter()
        .position(|(b, _, _)| *b == button)
        .expect("button is one of the GBA's")
}

/// Connected gamepads, if this machine can read them.
pub struct Gamepads(Gilrs);

impl Gamepads {
    pub fn new() -> Option<Self> {
        Gilrs::new().ok().map(Self)
    }

    /// The buttons held across all connected gamepads.
    pub fn pressed(&mut self) -> u32 {
        while self.0.next_event().is_some() {}
        self.0
            .gamepads()
            .map(|(_, pad)| {
                let x = pad.axis_data(Axis::LeftStickX).map_or(0.0, |a| a.value());
                let y = pad.axis_data(Axis::LeftStickY).map_or(0.0, |a| a.value());
                gamepad_mask(|button| pad.is_pressed(button), x, y)
            })
            .fold(0, |mask, pad| mask | pad)
    }

    /// The name of the first connected gamepad.
    pub fn name(&mut self) -> Option<String> {
        while self.0.next_event().is_some() {}
        self.0
            .gamepads()
            .next()
            .map(|(_, pad)| pad.name().to_string())
    }
}

/// Maps a standard gamepad by position, so A and B sit where they do on a GBA.
fn gamepad_mask(is_pressed: impl Fn(Button) -> bool, stick_x: f32, stick_y: f32) -> u32 {
    let buttons = [
        (Button::East, keys::A),
        (Button::South, keys::B),
        (Button::LeftTrigger, keys::L),
        (Button::RightTrigger, keys::R),
        (Button::Start, keys::START),
        (Button::Select, keys::SELECT),
        (Button::DPadUp, keys::UP),
        (Button::DPadDown, keys::DOWN),
        (Button::DPadLeft, keys::LEFT),
        (Button::DPadRight, keys::RIGHT),
    ];
    let mut mask = buttons
        .iter()
        .filter(|(button, _)| is_pressed(*button))
        .fold(0, |mask, (_, key)| mask | key);
    if stick_x > STICK_THRESHOLD {
        mask |= keys::RIGHT;
    } else if stick_x < -STICK_THRESHOLD {
        mask |= keys::LEFT;
    }
    if stick_y > STICK_THRESHOLD {
        mask |= keys::UP;
    } else if stick_y < -STICK_THRESHOLD {
        mask |= keys::DOWN;
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_documented_keys() {
        let bindings = KeyBindings::default();
        assert_eq!(bindings.key(keys::A), Key::Z);
        assert_eq!(bindings.key(keys::START), Key::Enter);
        assert_eq!(bindings.pressed(|key| key == Key::X), keys::B);
    }

    #[test]
    fn assigning_a_taken_key_swaps_the_two_buttons() {
        let mut bindings = KeyBindings::default();
        bindings.assign(keys::A, Key::X);
        assert_eq!(bindings.key(keys::A), Key::X);
        assert_eq!(bindings.key(keys::B), Key::Z);
    }

    #[test]
    fn several_keys_combine() {
        let bindings = KeyBindings::default();
        let mask = bindings.pressed(|key| key == Key::Z || key == Key::ArrowUp);
        assert_eq!(mask, keys::A | keys::UP);
    }

    #[test]
    fn bindings_survive_being_written_and_read() {
        let mut bindings = KeyBindings::default();
        bindings.assign(keys::START, Key::Q);
        bindings.assign(keys::A, Key::Space);
        assert_eq!(KeyBindings::from_text(&bindings.to_text()), bindings);
    }

    #[test]
    fn unusable_settings_fall_back_to_defaults() {
        let defaults = KeyBindings::default();
        assert_eq!(KeyBindings::from_text(""), defaults);
        assert_eq!(KeyBindings::from_text("nonsense;A=NotAKey;Z=Q"), defaults);
        assert_eq!(KeyBindings::from_text("A=F5"), defaults);
    }

    #[test]
    fn gamepad_buttons_sit_where_they_do_on_a_gba() {
        assert_eq!(gamepad_mask(|b| b == Button::East, 0.0, 0.0), keys::A);
        assert_eq!(gamepad_mask(|b| b == Button::South, 0.0, 0.0), keys::B);
        assert_eq!(gamepad_mask(|b| b == Button::DPadUp, 0.0, 0.0), keys::UP);
    }

    #[test]
    fn the_stick_acts_as_a_d_pad_past_its_threshold() {
        assert_eq!(gamepad_mask(|_| false, 0.2, -0.2), 0);
        assert_eq!(gamepad_mask(|_| false, 0.9, 0.0), keys::RIGHT);
        assert_eq!(gamepad_mask(|_| false, -0.9, 0.9), keys::LEFT | keys::UP);
        assert_eq!(gamepad_mask(|_| false, 0.0, -0.9), keys::DOWN);
    }
}
