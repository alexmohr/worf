//! Conversion of GDK keys and modifiers into worf's key types.

use std::collections::HashSet;

use gdk4::{Display, prelude::DisplayExtManual};

use crate::config::{Key, KeyDetectionType};

impl From<gtk4::gdk::Key> for Key {
    fn from(value: gdk4::Key) -> Self {
        match value {
            // Letters
            gdk4::Key::A => Key::A,
            gdk4::Key::B => Key::B,
            gdk4::Key::C => Key::C,
            gdk4::Key::D => Key::D,
            gdk4::Key::E => Key::E,
            gdk4::Key::F => Key::F,
            gdk4::Key::G => Key::G,
            gdk4::Key::H => Key::H,
            gdk4::Key::I => Key::I,
            gdk4::Key::J => Key::J,
            gdk4::Key::K => Key::K,
            gdk4::Key::L => Key::L,
            gdk4::Key::M => Key::M,
            gdk4::Key::N => Key::N,
            gdk4::Key::O => Key::O,
            gdk4::Key::P => Key::P,
            gdk4::Key::Q => Key::Q,
            gdk4::Key::R => Key::R,
            gdk4::Key::S => Key::S,
            gdk4::Key::T => Key::T,
            gdk4::Key::U => Key::U,
            gdk4::Key::V => Key::V,
            gdk4::Key::W => Key::W,
            gdk4::Key::X => Key::X,
            gdk4::Key::Y => Key::Y,
            gdk4::Key::Z => Key::Z,

            // Numbers
            gdk4::Key::_0 => Key::Num0,
            gdk4::Key::_1 => Key::Num1,
            gdk4::Key::_2 => Key::Num2,
            gdk4::Key::_3 => Key::Num3,
            gdk4::Key::_4 => Key::Num4,
            gdk4::Key::_5 => Key::Num5,
            gdk4::Key::_6 => Key::Num6,
            gdk4::Key::_7 => Key::Num7,
            gdk4::Key::_8 => Key::Num8,
            gdk4::Key::_9 => Key::Num9,

            // Function Keys
            gdk4::Key::F1 => Key::F1,
            gdk4::Key::F2 => Key::F2,
            gdk4::Key::F3 => Key::F3,
            gdk4::Key::F4 => Key::F4,
            gdk4::Key::F5 => Key::F5,
            gdk4::Key::F6 => Key::F6,
            gdk4::Key::F7 => Key::F7,
            gdk4::Key::F8 => Key::F8,
            gdk4::Key::F9 => Key::F9,
            gdk4::Key::F10 => Key::F10,
            gdk4::Key::F11 => Key::F11,
            gdk4::Key::F12 => Key::F12,

            // Navigation / Editing
            gdk4::Key::Escape => Key::Escape,
            gdk4::Key::Return | gdk4::Key::KP_Enter => Key::Enter,
            gdk4::Key::space => Key::Space,
            gdk4::Key::Tab => Key::Tab,
            gdk4::Key::BackSpace => Key::Backspace,
            gdk4::Key::Insert => Key::Insert,
            gdk4::Key::Delete => Key::Delete,
            gdk4::Key::Home => Key::Home,
            gdk4::Key::End => Key::End,
            gdk4::Key::Page_Up => Key::PageUp,
            gdk4::Key::Page_Down => Key::PageDown,
            gdk4::Key::Left => Key::Left,
            gdk4::Key::Right => Key::Right,
            gdk4::Key::Up => Key::Up,
            gdk4::Key::Down => Key::Down,

            // Special characters
            gdk4::Key::exclam => Key::Exclamation,
            gdk4::Key::at => Key::At,
            gdk4::Key::numbersign => Key::Hash,
            gdk4::Key::dollar => Key::Dollar,
            gdk4::Key::percent => Key::Percent,
            gdk4::Key::asciicircum => Key::Caret,
            gdk4::Key::ampersand => Key::Ampersand,
            gdk4::Key::asterisk => Key::Asterisk,
            gdk4::Key::parenleft => Key::LeftParen,
            gdk4::Key::parenright => Key::RightParen,
            gdk4::Key::minus => Key::Minus,
            gdk4::Key::underscore => Key::Underscore,
            gdk4::Key::equal => Key::Equal,
            gdk4::Key::plus => Key::Plus,
            gdk4::Key::bracketleft => Key::LeftBracket,
            gdk4::Key::bracketright => Key::RightBracket,
            gdk4::Key::braceleft => Key::LeftBrace,
            gdk4::Key::braceright => Key::RightBrace,
            gdk4::Key::backslash => Key::Backslash,
            gdk4::Key::bar => Key::Pipe,
            gdk4::Key::semicolon => Key::Semicolon,
            gdk4::Key::colon => Key::Colon,
            gdk4::Key::apostrophe => Key::Apostrophe,
            gdk4::Key::quotedbl => Key::Quote,
            gdk4::Key::comma => Key::Comma,
            gdk4::Key::period => Key::Period,
            gdk4::Key::slash => Key::Slash,
            gdk4::Key::question => Key::Question,
            gdk4::Key::grave => Key::Grave,
            gdk4::Key::asciitilde => Key::Tilde,
            _ => Key::None,
        }
    }
}

impl From<u32> for Key {
    fn from(value: u32) -> Self {
        match value {
            // Letters
            38 => Key::A,
            56 => Key::B,
            54 => Key::C,
            40 => Key::D,
            26 => Key::E,
            41 => Key::F,
            42 => Key::G,
            43 => Key::H,
            31 => Key::I,
            44 => Key::J,
            45 => Key::K,
            46 => Key::L,
            58 => Key::M,
            57 => Key::N,
            32 => Key::O,
            33 => Key::P,
            24 => Key::Q,
            27 => Key::R,
            39 => Key::S,
            28 => Key::T,
            30 => Key::U,
            55 => Key::V,
            25 => Key::W,
            53 => Key::X,
            29 => Key::Y,
            52 => Key::Z,

            // Numbers
            10 => Key::Num1,
            11 => Key::Num2,
            12 => Key::Num3,
            13 => Key::Num4,
            14 => Key::Num5,
            15 => Key::Num6,
            16 => Key::Num7,
            17 => Key::Num8,
            18 => Key::Num9,
            19 => Key::Num0,

            // Function Keys
            67 => Key::F1,
            68 => Key::F2,
            69 => Key::F3,
            70 => Key::F4,
            71 => Key::F5,
            72 => Key::F6,
            73 => Key::F7,
            74 => Key::F8,
            75 => Key::F9,
            76 => Key::F10,
            77 => Key::F11,
            78 => Key::F12,

            // Navigation / Editing
            9 => Key::Escape,
            36 | 104 => Key::Enter,
            65 => Key::Space,
            23 => Key::Tab,
            22 => Key::Backspace,
            118 => Key::Insert,
            119 => Key::Delete,
            110 => Key::Home,
            115 => Key::End,
            112 => Key::PageUp,
            117 => Key::PageDown,
            113 => Key::Left,
            114 => Key::Right,
            111 => Key::Up,
            116 => Key::Down,

            // Special characters
            20 => Key::Exclamation,
            63 => Key::At,
            3 => Key::Hash,
            4 => Key::Dollar,
            5 => Key::Percent,
            6 => Key::Caret,
            7 => Key::Ampersand,
            8 => Key::Asterisk,
            34 => Key::LeftParen,
            35 => Key::RightParen,
            48 => Key::Minus,
            47 => Key::Underscore,
            21 => Key::Equal,
            49 => Key::Plus,
            51 => Key::Backslash,
            94 => Key::Pipe,
            50 => Key::Quote,
            59 => Key::Comma,
            60 => Key::Period,
            61 => Key::Slash,
            62 => Key::Question,
            96 => Key::Grave,
            97 => Key::Tilde,
            _ => Key::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Shift,
    Control,
    Alt,
    Super,
    Meta,
    CapsLock,
    None,
}

pub(super) fn modifiers_from_mask(mask: gdk4::ModifierType) -> HashSet<Modifier> {
    let mut modifiers = HashSet::new();

    if mask.contains(gdk4::ModifierType::SHIFT_MASK) {
        modifiers.insert(Modifier::Shift);
    }
    if mask.contains(gdk4::ModifierType::CONTROL_MASK) {
        modifiers.insert(Modifier::Control);
    }
    if mask.contains(gdk4::ModifierType::ALT_MASK) {
        modifiers.insert(Modifier::Alt);
    }
    if mask.contains(gdk4::ModifierType::SUPER_MASK) {
        modifiers.insert(Modifier::Super);
    }
    if mask.contains(gdk4::ModifierType::META_MASK) {
        modifiers.insert(Modifier::Meta);
    }
    if mask.contains(gdk4::ModifierType::LOCK_MASK) {
        modifiers.insert(Modifier::CapsLock);
    }

    if modifiers.is_empty() {
        modifiers.insert(Modifier::None);
    }

    modifiers
}

/// True if a modifier is held that turns a key press into a shortcut instead of text input.
pub(super) fn has_command_modifier(mask: gdk4::ModifierType) -> bool {
    mask.intersects(
        gdk4::ModifierType::CONTROL_MASK
            | gdk4::ModifierType::ALT_MASK
            | gdk4::ModifierType::SUPER_MASK
            | gdk4::ModifierType::META_MASK,
    )
}

/// Modifiers that are relevant to match a key binding.
/// Lock keys like `CapsLock` are ignored, so bindings work regardless of their state.
pub(super) fn significant_modifiers(modifiers: &HashSet<Modifier>) -> HashSet<Modifier> {
    modifiers
        .iter()
        .filter(|m| !matches!(m, Modifier::CapsLock | Modifier::None))
        .copied()
        .collect()
}

/// Returns the key the given hardware key code produces without any modifier.
pub(super) fn unshifted_key(key_code: u32) -> Option<gdk4::Key> {
    Display::default()?
        .map_keycode(key_code)?
        .into_iter()
        .find(|(keymap_key, _)| keymap_key.group() == 0 && keymap_key.level() == 0)
        .map(|(_, key)| key)
}

pub(super) fn is_key_match(
    key_opt: Option<Key>,
    key_detection_type: &KeyDetectionType,
    key_code: u32,
    gdk_key: gdk4::Key,
) -> bool {
    if let Some(key) = key_opt {
        if key_detection_type == &KeyDetectionType::Code {
            key == key_code.into()
        } else {
            key == gdk_key.to_upper().into()
        }
    } else {
        false
    }
}
