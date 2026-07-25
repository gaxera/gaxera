//! VirtIO-Input Keyboard and Mouse Driver Server Library.

#![no_std]

pub mod input;

pub use input::{ev_type, rel_code, FocusHandle, InputEvent, VirtioInputDriver, VirtioInputEvent};

#[cfg(test)]
mod tests {
    use super::*;
    use gaxera_abi::GaxObjectId;

    #[test]
    fn test_keyboard_key_event_decoding_and_focus_scoping() {
        let mut driver = VirtioInputDriver::new();
        let task1 = GaxObjectId::from_bytes([1; 16]);
        let task2 = GaxObjectId::from_bytes([2; 16]);
        let win = GaxObjectId::from_bytes([3; 16]);

        let raw_key = VirtioInputEvent {
            event_type: ev_type::EV_KEY,
            code: 30, // KEY_A
            value: 1, // Pressed
        };

        // 1. Unfocused task receives None
        assert!(driver.decode_event(&raw_key, task1).is_none());

        // 2. Set focus to task1
        driver.set_focus(FocusHandle::new(task1, win));
        let decoded = driver.decode_event(&raw_key, task1).unwrap();
        assert_eq!(
            decoded,
            InputEvent::Keyboard {
                key_code: 30,
                is_pressed: true
            }
        );

        // 3. Task2 remains blocked from task1's input stream
        assert!(driver.decode_event(&raw_key, task2).is_none());
    }

    #[test]
    fn test_pointer_relative_move_decoding() {
        let mut driver = VirtioInputDriver::new();
        let task = GaxObjectId::from_bytes([1; 16]);
        let win = GaxObjectId::from_bytes([2; 16]);
        driver.set_focus(FocusHandle::new(task, win));

        let raw_rel_x = VirtioInputEvent {
            event_type: ev_type::EV_REL,
            code: rel_code::REL_X,
            value: 15,
        };

        let decoded = driver.decode_event(&raw_rel_x, task).unwrap();
        assert_eq!(decoded, InputEvent::PointerMove { dx: 15, dy: 0 });
    }
}
