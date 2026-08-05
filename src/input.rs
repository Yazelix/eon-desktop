use orbit_protocol::session::{
    ClientMessage, FocusEvent, KeyAction, KeyEvent, Modifiers, MouseAction, MouseButton,
    MouseEvent, PhysicalKey,
};
use winit::{
    event::{ElementState, Ime, KeyEvent as WinitKeyEvent, MouseButton as WinitMouseButton},
    keyboard::{KeyCode, ModifiersState, PhysicalKey as WinitPhysicalKey},
};

/// Stateful translation from native events to Orbit-owned semantic values.
#[derive(Clone, Debug, Default)]
pub struct InputState {
    modifiers: Modifiers,
    composing: bool,
    preedit: String,
    cursor: (f32, f32),
    pressed_buttons: Vec<WinitMouseButton>,
}

impl InputState {
    pub fn set_modifiers(&mut self, modifiers: ModifiersState) {
        let mut result = Modifiers::empty();
        for (active, value) in [
            (modifiers.shift_key(), Modifiers::SHIFT),
            (modifiers.control_key(), Modifiers::CTRL),
            (modifiers.alt_key(), Modifiers::ALT),
            (modifiers.super_key(), Modifiers::SUPER),
        ] {
            if active {
                result = result.union(value);
            }
        }
        self.modifiers = result;
    }

    #[must_use]
    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    pub fn focus(&mut self, focused: bool) -> ClientMessage {
        if !focused {
            self.modifiers = Modifiers::empty();
            self.pressed_buttons.clear();
            self.clear_composition();
        }
        ClientMessage::Focus(if focused {
            FocusEvent::Gained
        } else {
            FocusEvent::Lost
        })
    }

    #[must_use]
    pub fn key(&self, event: &WinitKeyEvent) -> ClientMessage {
        let code = match event.physical_key {
            WinitPhysicalKey::Code(code) => Some(code),
            WinitPhysicalKey::Unidentified(_) => None,
        };
        ClientMessage::Key(KeyEvent {
            action: match (event.state, event.repeat) {
                (ElementState::Released, _) => KeyAction::Release,
                (ElementState::Pressed, true) => KeyAction::Repeat,
                (ElementState::Pressed, false) => KeyAction::Press,
            },
            key: code.map_or(PhysicalKey::UNIDENTIFIED, physical_key),
            modifiers: self.modifiers,
            consumed_modifiers: Modifiers::empty(),
            composing: self.composing,
            text: event.text.as_deref().and_then(key_text),
            unshifted_codepoint: code.and_then(unshifted_codepoint),
        })
    }

    /// Track native IME state and return a semantic commit when text is finalized.
    pub fn ime(&mut self, event: Ime) -> Option<ClientMessage> {
        match event {
            Ime::Enabled => None,
            Ime::Disabled => {
                self.clear_composition();
                None
            }
            Ime::Preedit(text, _) => {
                self.composing = !text.is_empty();
                self.preedit = text;
                None
            }
            Ime::Commit(text) => {
                self.clear_composition();
                let text = key_text(&text)?;
                Some(ClientMessage::Key(KeyEvent {
                    action: KeyAction::Press,
                    key: PhysicalKey::UNIDENTIFIED,
                    modifiers: self.modifiers,
                    consumed_modifiers: Modifiers::empty(),
                    composing: false,
                    text: Some(text),
                    unshifted_codepoint: None,
                }))
            }
        }
    }

    fn clear_composition(&mut self) {
        self.composing = false;
        self.preedit.clear();
    }

    pub fn move_pointer(&mut self, x: f64, y: f64) -> Option<ClientMessage> {
        let (x, y) = coordinates(x, y)?;
        self.cursor = (x, y);
        Some(ClientMessage::Mouse(MouseEvent {
            action: MouseAction::Motion,
            button: self.pressed_buttons.last().copied().map(mouse_button),
            modifiers: self.modifiers,
            x,
            y,
        }))
    }

    /// Keep semantic button transitions paired across presentation and queue gates.
    pub fn mouse_button(
        &mut self,
        state: ElementState,
        button: WinitMouseButton,
        accept_press: bool,
    ) -> Option<ClientMessage> {
        match state {
            ElementState::Pressed if !accept_press || self.pressed_buttons.contains(&button) => {
                return None;
            }
            ElementState::Released if !self.pressed_buttons.contains(&button) => return None,
            _ => {}
        }
        self.pressed_buttons.retain(|pressed| *pressed != button);
        let action = match state {
            ElementState::Pressed => {
                self.pressed_buttons.push(button);
                MouseAction::Press
            }
            ElementState::Released => MouseAction::Release,
        };
        let button = mouse_button(button);
        Some(ClientMessage::Mouse(MouseEvent {
            action,
            button: Some(button),
            modifiers: self.modifiers,
            x: self.cursor.0,
            y: self.cursor.1,
        }))
    }

    /// Forget a press that failed to enter the outbound queue.
    pub fn reject_mouse_press(&mut self, button: WinitMouseButton) {
        self.pressed_buttons.retain(|pressed| *pressed != button);
    }

    #[must_use]
    pub fn wheel(&self, horizontal: f32, vertical: f32) -> Option<ClientMessage> {
        let button = if vertical > 0.0 {
            MouseButton::Four
        } else if vertical < 0.0 {
            MouseButton::Five
        } else if horizontal < 0.0 {
            MouseButton::Six
        } else if horizontal > 0.0 {
            MouseButton::Seven
        } else {
            return None;
        };
        Some(ClientMessage::Mouse(MouseEvent {
            action: MouseAction::Press,
            button: Some(button),
            modifiers: self.modifiers,
            x: self.cursor.0,
            y: self.cursor.1,
        }))
    }
}

/// Map winit's physical identity directly into Orbit's accepted semantic type.
#[must_use]
fn physical_key(code: KeyCode) -> PhysicalKey {
    use KeyCode as W;
    match code {
        W::Backquote => PhysicalKey::BACKQUOTE,
        W::Backslash => PhysicalKey::BACKSLASH,
        W::BracketLeft => PhysicalKey::BRACKET_LEFT,
        W::BracketRight => PhysicalKey::BRACKET_RIGHT,
        W::Comma => PhysicalKey::COMMA,
        W::Digit0 => PhysicalKey::DIGIT_0,
        W::Digit1 => PhysicalKey::DIGIT_1,
        W::Digit2 => PhysicalKey::DIGIT_2,
        W::Digit3 => PhysicalKey::DIGIT_3,
        W::Digit4 => PhysicalKey::DIGIT_4,
        W::Digit5 => PhysicalKey::DIGIT_5,
        W::Digit6 => PhysicalKey::DIGIT_6,
        W::Digit7 => PhysicalKey::DIGIT_7,
        W::Digit8 => PhysicalKey::DIGIT_8,
        W::Digit9 => PhysicalKey::DIGIT_9,
        W::Equal => PhysicalKey::EQUAL,
        W::IntlBackslash => PhysicalKey::INTL_BACKSLASH,
        W::IntlRo => PhysicalKey::INTL_RO,
        W::IntlYen => PhysicalKey::INTL_YEN,
        W::KeyA => PhysicalKey::A,
        W::KeyB => PhysicalKey::B,
        W::KeyC => PhysicalKey::C,
        W::KeyD => PhysicalKey::D,
        W::KeyE => PhysicalKey::E,
        W::KeyF => PhysicalKey::F,
        W::KeyG => PhysicalKey::G,
        W::KeyH => PhysicalKey::H,
        W::KeyI => PhysicalKey::I,
        W::KeyJ => PhysicalKey::J,
        W::KeyK => PhysicalKey::K,
        W::KeyL => PhysicalKey::L,
        W::KeyM => PhysicalKey::M,
        W::KeyN => PhysicalKey::N,
        W::KeyO => PhysicalKey::O,
        W::KeyP => PhysicalKey::P,
        W::KeyQ => PhysicalKey::Q,
        W::KeyR => PhysicalKey::R,
        W::KeyS => PhysicalKey::S,
        W::KeyT => PhysicalKey::T,
        W::KeyU => PhysicalKey::U,
        W::KeyV => PhysicalKey::V,
        W::KeyW => PhysicalKey::W,
        W::KeyX => PhysicalKey::X,
        W::KeyY => PhysicalKey::Y,
        W::KeyZ => PhysicalKey::Z,
        W::Minus => PhysicalKey::MINUS,
        W::Period => PhysicalKey::PERIOD,
        W::Quote => PhysicalKey::QUOTE,
        W::Semicolon => PhysicalKey::SEMICOLON,
        W::Slash => PhysicalKey::SLASH,
        W::AltLeft => PhysicalKey::ALT_LEFT,
        W::AltRight => PhysicalKey::ALT_RIGHT,
        W::Backspace => PhysicalKey::BACKSPACE,
        W::CapsLock => PhysicalKey::CAPS_LOCK,
        W::ContextMenu => PhysicalKey::CONTEXT_MENU,
        W::ControlLeft => PhysicalKey::CONTROL_LEFT,
        W::ControlRight => PhysicalKey::CONTROL_RIGHT,
        W::Enter => PhysicalKey::ENTER,
        W::SuperLeft => PhysicalKey::META_LEFT,
        W::SuperRight => PhysicalKey::META_RIGHT,
        W::ShiftLeft => PhysicalKey::SHIFT_LEFT,
        W::ShiftRight => PhysicalKey::SHIFT_RIGHT,
        W::Space => PhysicalKey::SPACE,
        W::Tab => PhysicalKey::TAB,
        W::Convert => PhysicalKey::CONVERT,
        W::KanaMode => PhysicalKey::KANA_MODE,
        W::NonConvert => PhysicalKey::NON_CONVERT,
        W::Delete => PhysicalKey::DELETE,
        W::End => PhysicalKey::END,
        W::Help => PhysicalKey::HELP,
        W::Home => PhysicalKey::HOME,
        W::Insert => PhysicalKey::INSERT,
        W::PageDown => PhysicalKey::PAGE_DOWN,
        W::PageUp => PhysicalKey::PAGE_UP,
        W::ArrowDown => PhysicalKey::ARROW_DOWN,
        W::ArrowLeft => PhysicalKey::ARROW_LEFT,
        W::ArrowRight => PhysicalKey::ARROW_RIGHT,
        W::ArrowUp => PhysicalKey::ARROW_UP,
        W::NumLock => PhysicalKey::NUM_LOCK,
        W::Numpad0 => PhysicalKey::NUMPAD_0,
        W::Numpad1 => PhysicalKey::NUMPAD_1,
        W::Numpad2 => PhysicalKey::NUMPAD_2,
        W::Numpad3 => PhysicalKey::NUMPAD_3,
        W::Numpad4 => PhysicalKey::NUMPAD_4,
        W::Numpad5 => PhysicalKey::NUMPAD_5,
        W::Numpad6 => PhysicalKey::NUMPAD_6,
        W::Numpad7 => PhysicalKey::NUMPAD_7,
        W::Numpad8 => PhysicalKey::NUMPAD_8,
        W::Numpad9 => PhysicalKey::NUMPAD_9,
        W::NumpadAdd => PhysicalKey::NUMPAD_ADD,
        W::NumpadBackspace => PhysicalKey::NUMPAD_BACKSPACE,
        W::NumpadClear => PhysicalKey::NUMPAD_CLEAR,
        W::NumpadClearEntry => PhysicalKey::NUMPAD_CLEAR_ENTRY,
        W::NumpadComma => PhysicalKey::NUMPAD_COMMA,
        W::NumpadDecimal => PhysicalKey::NUMPAD_DECIMAL,
        W::NumpadDivide => PhysicalKey::NUMPAD_DIVIDE,
        W::NumpadEnter => PhysicalKey::NUMPAD_ENTER,
        W::NumpadEqual => PhysicalKey::NUMPAD_EQUAL,
        W::NumpadMemoryAdd => PhysicalKey::NUMPAD_MEMORY_ADD,
        W::NumpadMemoryClear => PhysicalKey::NUMPAD_MEMORY_CLEAR,
        W::NumpadMemoryRecall => PhysicalKey::NUMPAD_MEMORY_RECALL,
        W::NumpadMemoryStore => PhysicalKey::NUMPAD_MEMORY_STORE,
        W::NumpadMemorySubtract => PhysicalKey::NUMPAD_MEMORY_SUBTRACT,
        W::NumpadMultiply | W::NumpadStar => PhysicalKey::NUMPAD_MULTIPLY,
        W::NumpadParenLeft => PhysicalKey::NUMPAD_PAREN_LEFT,
        W::NumpadParenRight => PhysicalKey::NUMPAD_PAREN_RIGHT,
        W::NumpadSubtract => PhysicalKey::NUMPAD_SUBTRACT,
        W::Escape => PhysicalKey::ESCAPE,
        W::Fn => PhysicalKey::FN,
        W::FnLock => PhysicalKey::FN_LOCK,
        W::PrintScreen => PhysicalKey::PRINT_SCREEN,
        W::ScrollLock => PhysicalKey::SCROLL_LOCK,
        W::Pause => PhysicalKey::PAUSE,
        W::BrowserBack => PhysicalKey::BROWSER_BACK,
        W::BrowserFavorites => PhysicalKey::BROWSER_FAVORITES,
        W::BrowserForward => PhysicalKey::BROWSER_FORWARD,
        W::BrowserHome => PhysicalKey::BROWSER_HOME,
        W::BrowserRefresh => PhysicalKey::BROWSER_REFRESH,
        W::BrowserSearch => PhysicalKey::BROWSER_SEARCH,
        W::BrowserStop => PhysicalKey::BROWSER_STOP,
        W::Eject => PhysicalKey::EJECT,
        W::LaunchApp1 => PhysicalKey::LAUNCH_APP_1,
        W::LaunchApp2 => PhysicalKey::LAUNCH_APP_2,
        W::LaunchMail => PhysicalKey::LAUNCH_MAIL,
        W::MediaPlayPause => PhysicalKey::MEDIA_PLAY_PAUSE,
        W::MediaSelect => PhysicalKey::MEDIA_SELECT,
        W::MediaStop => PhysicalKey::MEDIA_STOP,
        W::MediaTrackNext => PhysicalKey::MEDIA_TRACK_NEXT,
        W::MediaTrackPrevious => PhysicalKey::MEDIA_TRACK_PREVIOUS,
        W::Power => PhysicalKey::POWER,
        W::Sleep => PhysicalKey::SLEEP,
        W::AudioVolumeDown => PhysicalKey::AUDIO_VOLUME_DOWN,
        W::AudioVolumeMute => PhysicalKey::AUDIO_VOLUME_MUTE,
        W::AudioVolumeUp => PhysicalKey::AUDIO_VOLUME_UP,
        W::WakeUp => PhysicalKey::WAKE_UP,
        W::Copy => PhysicalKey::COPY,
        W::Cut => PhysicalKey::CUT,
        W::Paste => PhysicalKey::PASTE,
        W::F1 => PhysicalKey::F1,
        W::F2 => PhysicalKey::F2,
        W::F3 => PhysicalKey::F3,
        W::F4 => PhysicalKey::F4,
        W::F5 => PhysicalKey::F5,
        W::F6 => PhysicalKey::F6,
        W::F7 => PhysicalKey::F7,
        W::F8 => PhysicalKey::F8,
        W::F9 => PhysicalKey::F9,
        W::F10 => PhysicalKey::F10,
        W::F11 => PhysicalKey::F11,
        W::F12 => PhysicalKey::F12,
        W::F13 => PhysicalKey::F13,
        W::F14 => PhysicalKey::F14,
        W::F15 => PhysicalKey::F15,
        W::F16 => PhysicalKey::F16,
        W::F17 => PhysicalKey::F17,
        W::F18 => PhysicalKey::F18,
        W::F19 => PhysicalKey::F19,
        W::F20 => PhysicalKey::F20,
        W::F21 => PhysicalKey::F21,
        W::F22 => PhysicalKey::F22,
        W::F23 => PhysicalKey::F23,
        W::F24 => PhysicalKey::F24,
        W::F25 => PhysicalKey::F25,
        _ => PhysicalKey::UNIDENTIFIED,
    }
}

fn unshifted_codepoint(code: KeyCode) -> Option<char> {
    use KeyCode as W;
    Some(match code {
        W::Backquote => '`',
        W::Backslash | W::IntlBackslash | W::IntlYen => '\\',
        W::BracketLeft => '[',
        W::BracketRight => ']',
        W::Comma => ',',
        W::Digit0 => '0',
        W::Digit1 => '1',
        W::Digit2 => '2',
        W::Digit3 => '3',
        W::Digit4 => '4',
        W::Digit5 => '5',
        W::Digit6 => '6',
        W::Digit7 => '7',
        W::Digit8 => '8',
        W::Digit9 => '9',
        W::Equal => '=',
        W::KeyA => 'a',
        W::KeyB => 'b',
        W::KeyC => 'c',
        W::KeyD => 'd',
        W::KeyE => 'e',
        W::KeyF => 'f',
        W::KeyG => 'g',
        W::KeyH => 'h',
        W::KeyI => 'i',
        W::KeyJ => 'j',
        W::KeyK => 'k',
        W::KeyL => 'l',
        W::KeyM => 'm',
        W::KeyN => 'n',
        W::KeyO => 'o',
        W::KeyP => 'p',
        W::KeyQ => 'q',
        W::KeyR => 'r',
        W::KeyS => 's',
        W::KeyT => 't',
        W::KeyU => 'u',
        W::KeyV => 'v',
        W::KeyW => 'w',
        W::KeyX => 'x',
        W::KeyY => 'y',
        W::KeyZ => 'z',
        W::Minus => '-',
        W::Period => '.',
        W::Quote => '\'',
        W::Semicolon => ';',
        W::Slash | W::IntlRo => '/',
        W::Space => ' ',
        _ => return None,
    })
}

fn coordinates(x: f64, y: f64) -> Option<(f32, f32)> {
    let maximum = f64::from(u16::MAX);
    (x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0 && x <= maximum && y <= maximum)
        .then_some((x as f32, y as f32))
}

fn key_text(text: &str) -> Option<String> {
    (!text.is_empty()
        && text.len() <= orbit_protocol::session::MAX_KEY_TEXT_BYTES
        && !text.chars().any(|character| {
            matches!(
                character,
                '\0'..='\u{1f}' | '\u{7f}' | '\u{f700}'..='\u{f8ff}'
            )
        }))
    .then(|| text.to_owned())
}

fn mouse_button(button: winit::event::MouseButton) -> MouseButton {
    match button {
        winit::event::MouseButton::Left => MouseButton::Left,
        winit::event::MouseButton::Middle => MouseButton::Middle,
        winit::event::MouseButton::Right => MouseButton::Right,
        winit::event::MouseButton::Back => MouseButton::Eight,
        winit::event::MouseButton::Forward => MouseButton::Nine,
        winit::event::MouseButton::Other(10) => MouseButton::Ten,
        winit::event::MouseButton::Other(11) => MouseButton::Eleven,
        winit::event::MouseButton::Other(_) => MouseButton::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_layout_independent_physical_keys() {
        assert_eq!(physical_key(KeyCode::KeyA), PhysicalKey::A);
        assert_eq!(physical_key(KeyCode::IntlRo), PhysicalKey::INTL_RO);
        assert_eq!(
            physical_key(KeyCode::NumpadEnter),
            PhysicalKey::NUMPAD_ENTER
        );
        assert_eq!(physical_key(KeyCode::F25), PhysicalKey::F25);
        assert_eq!(physical_key(KeyCode::Lang1), PhysicalKey::UNIDENTIFIED);
    }

    #[test]
    fn ime_sends_only_committed_text() {
        let mut input = InputState::default();
        assert!(input.ime(Ime::Preedit("a".into(), Some((1, 1)))).is_none());
        assert_eq!(input.preedit(), "a");
        let Some(ClientMessage::Key(event)) = input.ime(Ime::Commit("啊".into())) else {
            panic!("expected a semantic key commit");
        };
        assert_eq!(event.key, PhysicalKey::UNIDENTIFIED);
        assert_eq!(event.text.as_deref(), Some("啊"));
        assert!(!event.composing);
        assert!(input.preedit().is_empty());
    }

    #[test]
    fn focus_loss_clears_transient_native_input_state() {
        let mut input = InputState::default();
        input.set_modifiers(ModifiersState::CONTROL);
        input.ime(Ime::Preedit("compose".into(), None));
        let _ = input.mouse_button(ElementState::Pressed, WinitMouseButton::Left, true);

        assert_eq!(input.focus(false), ClientMessage::Focus(FocusEvent::Lost));
        assert!(input.preedit().is_empty());
        assert!(!input.composing);
        let Some(ClientMessage::Mouse(event)) = input.move_pointer(10.0, 20.0) else {
            panic!("expected semantic pointer motion");
        };
        assert_eq!(event.button, None);
        assert_eq!(event.modifiers, Modifiers::empty());
    }

    #[test]
    fn pointer_motion_keeps_the_remaining_pressed_button() {
        for (released, expected) in [
            (WinitMouseButton::Left, MouseButton::Right),
            (WinitMouseButton::Right, MouseButton::Left),
        ] {
            let mut input = InputState::default();
            let _ = input.mouse_button(ElementState::Pressed, WinitMouseButton::Left, true);
            let _ = input.mouse_button(ElementState::Pressed, WinitMouseButton::Right, true);
            let _ = input.mouse_button(ElementState::Released, released, true);
            let Some(ClientMessage::Mouse(event)) = input.move_pointer(10.0, 20.0) else {
                panic!("expected semantic pointer motion");
            };
            assert_eq!(event.button, Some(expected));
        }
    }

    #[test]
    fn button_pairing_survives_presentation_and_queue_gates() {
        let mut input = InputState::default();
        {
            let mut button =
                |state, button, accept_press| input.mouse_button(state, button, accept_press);
            assert!(button(ElementState::Pressed, WinitMouseButton::Left, false).is_none());
            assert!(button(ElementState::Released, WinitMouseButton::Left, true).is_none());
            assert!(button(ElementState::Pressed, WinitMouseButton::Right, true).is_some());
            assert!(button(ElementState::Pressed, WinitMouseButton::Right, true).is_none());
        }
        input.reject_mouse_press(WinitMouseButton::Right);
        let Some(ClientMessage::Mouse(motion)) = input.move_pointer(10.0, 20.0) else {
            panic!("expected semantic pointer motion");
        };
        assert_eq!(motion.button, None);

        assert!(
            input
                .mouse_button(ElementState::Pressed, WinitMouseButton::Right, true)
                .is_some()
        );
        assert!(matches!(
            input.mouse_button(ElementState::Released, WinitMouseButton::Right, false),
            Some(ClientMessage::Mouse(MouseEvent {
                action: MouseAction::Release,
                button: Some(MouseButton::Right),
                ..
            }))
        ));
    }

    #[test]
    fn pointer_coordinates_stay_inside_orbit_domain() {
        let mut input = InputState::default();
        assert!(input.move_pointer(10.5, 20.25).is_some());
        assert!(input.move_pointer(-1.0, 0.0).is_none());
        assert!(input.move_pointer(f64::INFINITY, 0.0).is_none());
        assert!(input.move_pointer(f64::from(u16::MAX) + 1.0, 0.0).is_none());
    }

    #[test]
    fn native_control_text_does_not_replace_physical_key_meaning() {
        assert_eq!(key_text("a"), Some("a".into()));
        assert_eq!(key_text("界"), Some("界".into()));
        assert_eq!(key_text("\r"), None);
        assert_eq!(key_text("\u{f700}"), None);
    }
}
