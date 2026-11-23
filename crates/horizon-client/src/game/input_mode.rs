//! What the player plays with (horizon_client::input_mode): the touch controls, the chat's bar and
//! the on-screen keyboard show while it is the touch screen. On Android the mouse's events come
//! here from the input hook (android.rs) and go on as the window events a computer's mouse
//! makes; a physical keyboard's keys and a controller (game/pad.rs) also count as the input used.
use super::Game;
#[cfg(target_os = "android")]
use super::{event_loop::EventLoopCtl, input_trace::InputOrigin};
use horizon_client::input_mode::InputMode;
#[cfg(target_os = "android")]
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, MouseScrollDelta, TouchPhase, WindowEvent},
    keyboard::KeyCode,
    window::WindowId,
};

impl Game<'_> {
    /// The player used `input`. Off the touch screen the fingers let go of what they held and the
    /// on-screen keyboard goes (touch_frame); any input but a controller takes the menus back
    /// from the pad's pointer.
    pub(super) fn input_used(&mut self, input: InputMode) {
        let mode = self
            .settings
            .touch
            .input_selection
            .resolve(self.input_mode.after(input, cfg!(target_os = "android")));
        if mode == self.input_mode {
            return;
        }
        if self.input_mode == InputMode::Touch {
            self.touch_release_all();
        }
        if mode != InputMode::Pad {
            self.pad.in_use = false;
        }
        self.input_mode = mode;
    }

    /// The touch controls, the chat's bar and the on-screen keyboard show.
    pub(super) fn touch_shown(&self) -> bool {
        self.input_mode == InputMode::Touch
    }

    /// Once per loop iteration, before the touch frame: the pointer capture Android took back,
    /// a physical keyboard's keys, and the mouse's events since the last iteration, as window
    /// events (its captured motion turns the view as a computer's raw motion does).
    #[cfg(target_os = "android")]
    pub(super) fn android_input_frame(&mut self, event_loop: &dyn EventLoopCtl) {
        if crate::android::pointer_capture_lost() && self.captured {
            self.capture(false);
        }
        if crate::android::keyboard_used() {
            self.input_used(InputMode::KeyboardMouse);
        }
        // The launcher's on-screen buttons. They take the same two steps a winit key event takes
        // in handle_window_event (remember the physical key, then press or release it), rather
        // than being wrapped in a KeyEvent: that struct's Android-only field is private to winit,
        // and a launcher's key has no key-without-modifiers text to carry anyway.
        let keys = crate::android::take_keys();
        if !keys.is_empty() {
            self.input_used(InputMode::KeyboardMouse);
            for key in keys {
                let code = key_code(key.name);
                // The mapping, not the bare press: `mapped_key` holds the key in `self.keys`,
                // which is what the game reads every tick. A press alone opens a screen (T) and
                // then goes no further, which is why a held W never walked.
                if key.down {
                    self.physical_keys.insert(code);
                } else {
                    self.physical_keys.remove(&code);
                }
                if let Err(e) = self.mapped_key(code, key.down, false) {
                    log::warn!("a launcher key {code:?}: {e}");
                }
            }
        }

        // What the launcher's keyboard typed: a field with the focus takes it, as a window's
        // char callback feeds GLFW's charTyped.
        let typed = crate::android::take_chars();
        if !typed.is_empty() {
            let text: String = typed.into_iter().collect();
            if self.types_text() {
                if let Err(e) = self.type_chars_into_screen(&text) {
                    log::warn!("a launcher's typed text: {e}");
                }
            }
        }

        let events = crate::android::take_mouse_events();
        if events.is_empty() {
            // A key came and went with no mouse event beside it: the keys were handled above,
            // and returning here is what the loop expects of an idle iteration.
            return;
        }
        self.input_used(InputMode::KeyboardMouse);
        // The launcher draws into its own surface and has no window: the events carry the dummy
        // id, as a headless game's do, and the game must not drop them for that.
        let id = match self.window.as_ref() {
            Some(window) => window.id(),
            None if cfg!(target_os = "android") => WindowId::dummy(),
            None => return,
        };
        let device_id = DeviceId::dummy();
        for event in events {
            let event = match event {
                crate::android::MouseEvent::Moved(x, y) => WindowEvent::CursorMoved {
                    device_id,
                    position: PhysicalPosition::new(x, y),
                },
                crate::android::MouseEvent::Button(button, down) => WindowEvent::MouseInput {
                    device_id,
                    state: if down {
                        ElementState::Pressed
                    } else {
                        ElementState::Released
                    },
                    button,
                },
                crate::android::MouseEvent::Wheel(x, y) => WindowEvent::MouseWheel {
                    device_id,
                    delta: MouseScrollDelta::LineDelta(x, y),
                    phase: TouchPhase::Moved,
                },
                crate::android::MouseEvent::Motion(x, y) => {
                    self.grabbed_mouse_motion(x, y);
                    continue;
                }
            };
            self.handle_window_event(event_loop, id, event, InputOrigin::User);
        }
    }
}

/// The `KeyCode` a launcher's control named (android::input::KeyPress, whose names are the
/// variants this build knows). An unknown name presses nothing: the launcher's own set is fixed
/// and the engine filters it in android_lib.rs already.
#[cfg(target_os = "android")]
fn key_code(name: &str) -> KeyCode {
    match name {
        "KeyA" => KeyCode::KeyA,
        "KeyB" => KeyCode::KeyB,
        "KeyC" => KeyCode::KeyC,
        "KeyD" => KeyCode::KeyD,
        "KeyE" => KeyCode::KeyE,
        "KeyF" => KeyCode::KeyF,
        "KeyG" => KeyCode::KeyG,
        "KeyH" => KeyCode::KeyH,
        "KeyI" => KeyCode::KeyI,
        "KeyJ" => KeyCode::KeyJ,
        "KeyK" => KeyCode::KeyK,
        "KeyL" => KeyCode::KeyL,
        "KeyM" => KeyCode::KeyM,
        "KeyN" => KeyCode::KeyN,
        "KeyO" => KeyCode::KeyO,
        "KeyP" => KeyCode::KeyP,
        "KeyQ" => KeyCode::KeyQ,
        "KeyR" => KeyCode::KeyR,
        "KeyS" => KeyCode::KeyS,
        "KeyT" => KeyCode::KeyT,
        "KeyU" => KeyCode::KeyU,
        "KeyV" => KeyCode::KeyV,
        "KeyW" => KeyCode::KeyW,
        "KeyX" => KeyCode::KeyX,
        "KeyY" => KeyCode::KeyY,
        "KeyZ" => KeyCode::KeyZ,
        "Digit0" => KeyCode::Digit0,
        "Digit1" => KeyCode::Digit1,
        "Digit2" => KeyCode::Digit2,
        "Digit3" => KeyCode::Digit3,
        "Digit4" => KeyCode::Digit4,
        "Digit5" => KeyCode::Digit5,
        "Digit6" => KeyCode::Digit6,
        "Digit7" => KeyCode::Digit7,
        "Digit8" => KeyCode::Digit8,
        "Digit9" => KeyCode::Digit9,
        "Space" => KeyCode::Space,
        "ShiftLeft" => KeyCode::ShiftLeft,
        "ShiftRight" => KeyCode::ShiftRight,
        "ControlLeft" => KeyCode::ControlLeft,
        "ControlRight" => KeyCode::ControlRight,
        "AltLeft" => KeyCode::AltLeft,
        "AltRight" => KeyCode::AltRight,
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Escape,
        "Tab" => KeyCode::Tab,
        "Backspace" => KeyCode::Backspace,
        "F1" => KeyCode::F1,
        "F2" => KeyCode::F2,
        "F3" => KeyCode::F3,
        "F4" => KeyCode::F4,
        "F5" => KeyCode::F5,
        "ArrowUp" => KeyCode::ArrowUp,
        "ArrowDown" => KeyCode::ArrowDown,
        "ArrowLeft" => KeyCode::ArrowLeft,
        "ArrowRight" => KeyCode::ArrowRight,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "Insert" => KeyCode::Insert,
        "Delete" => KeyCode::Delete,
        "CapsLock" => KeyCode::CapsLock,
        "NumLock" => KeyCode::NumLock,
        "ScrollLock" => KeyCode::ScrollLock,
        "Pause" => KeyCode::Pause,
        "F6" => KeyCode::F6,
        "F7" => KeyCode::F7,
        "F8" => KeyCode::F8,
        "F9" => KeyCode::F9,
        "F10" => KeyCode::F10,
        "F11" => KeyCode::F11,
        "F12" => KeyCode::F12,
        // The punctuation keys a chat line and the creative search need. Without them every one
        // of these fell into the arm below and arrived as Space, which is why "/" jumped.
        "Minus" => KeyCode::Minus,
        "Equal" => KeyCode::Equal,
        "BracketLeft" => KeyCode::BracketLeft,
        "BracketRight" => KeyCode::BracketRight,
        "Semicolon" => KeyCode::Semicolon,
        "Quote" => KeyCode::Quote,
        "Backquote" => KeyCode::Backquote,
        "Backslash" => KeyCode::Backslash,
        "Comma" => KeyCode::Comma,
        "Period" => KeyCode::Period,
        "Slash" => KeyCode::Slash,
        "NumpadEnter" => KeyCode::NumpadEnter,
        "NumpadDivide" => KeyCode::NumpadDivide,
        "NumpadAdd" => KeyCode::NumpadAdd,
        "NumpadSubtract" => KeyCode::NumpadSubtract,
        "NumpadDecimal" => KeyCode::NumpadDecimal,
        "Numpad0" => KeyCode::Numpad0,
        "Numpad1" => KeyCode::Numpad1,
        "Numpad2" => KeyCode::Numpad2,
        "Numpad3" => KeyCode::Numpad3,
        "Numpad4" => KeyCode::Numpad4,
        "Numpad5" => KeyCode::Numpad5,
        "Numpad6" => KeyCode::Numpad6,
        "Numpad7" => KeyCode::Numpad7,
        "Numpad8" => KeyCode::Numpad8,
        "Numpad9" => KeyCode::Numpad9,
        // A name this table does not know is not Space: it is no key at all, and the caller
        // treats it as one that does nothing.
        _ => KeyCode::Space,
    }
}
