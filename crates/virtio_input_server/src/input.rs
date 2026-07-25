//! VirtIO-Input Keyboard and Mouse Driver & Event Decoding.

use gaxera_abi::GaxObjectId;

/// Linux Input Event Types (`ev_type`).
pub mod ev_type {
    pub const EV_SYN: u16 = 0x00;
    pub const EV_KEY: u16 = 0x01;
    pub const EV_REL: u16 = 0x02;
    pub const EV_ABS: u16 = 0x03;
}

/// Linux Input Axis Codes.
pub mod rel_code {
    pub const REL_X: u16 = 0x00;
    pub const REL_Y: u16 = 0x01;
    pub const REL_WHEEL: u16 = 0x08;
}

/// Raw VirtIO Input Event (8 bytes).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct VirtioInputEvent {
    pub event_type: u16,
    pub code: u16,
    pub value: u32,
}

/// High-Level Standardized Gaxera Input Handle.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum InputEvent {
    Keyboard { key_code: u16, is_pressed: bool },
    PointerMove { dx: i32, dy: i32 },
    PointerButton { button: u8, is_pressed: bool },
}

/// Focus Capability Handle (`FocusHandle`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FocusHandle {
    pub task_id: GaxObjectId,
    pub window_id: GaxObjectId,
    pub is_active: bool,
}

impl FocusHandle {
    pub fn new(task_id: GaxObjectId, window_id: GaxObjectId) -> Self {
        Self {
            task_id,
            window_id,
            is_active: true,
        }
    }
}

/// Ring-3 VirtIO-Input Driver Instance.
pub struct VirtioInputDriver {
    pub current_focus: Option<FocusHandle>,
}

impl Default for VirtioInputDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtioInputDriver {
    pub fn new() -> Self {
        Self {
            current_focus: None,
        }
    }

    pub fn set_focus(&mut self, focus: FocusHandle) {
        self.current_focus = Some(focus);
    }

    /// Decode raw `VirtioInputEvent` into standardized Gaxera `InputEvent`.
    /// Enforces capability focus scoping: returns None if process has no active focus.
    pub fn decode_event(
        &self,
        raw: &VirtioInputEvent,
        requesting_task: GaxObjectId,
    ) -> Option<InputEvent> {
        // Focus capability check: deny input if requesting task is unfocused
        let focus = self.current_focus.as_ref()?;
        if !focus.is_active || focus.task_id != requesting_task {
            return None;
        }

        match raw.event_type {
            ev_type::EV_KEY => {
                let is_pressed = raw.value != 0;
                if raw.code >= 0x110 && raw.code <= 0x112 {
                    // Mouse Buttons BTN_LEFT, BTN_RIGHT, BTN_MIDDLE
                    let button = (raw.code - 0x110) as u8;
                    Some(InputEvent::PointerButton { button, is_pressed })
                } else {
                    // Standard Keyboard Key
                    Some(InputEvent::Keyboard {
                        key_code: raw.code,
                        is_pressed,
                    })
                }
            }
            ev_type::EV_REL => match raw.code {
                rel_code::REL_X => Some(InputEvent::PointerMove {
                    dx: raw.value as i32,
                    dy: 0,
                }),
                rel_code::REL_Y => Some(InputEvent::PointerMove {
                    dx: 0,
                    dy: raw.value as i32,
                }),
                _ => None,
            },
            _ => None,
        }
    }
}
