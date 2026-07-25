//! End-to-End VirtIO Reference Platform Integration Test Suite.

use gaxera_abi::Handle;
use gaxera_abi::GaxObjectId;
use libgaxera::driver::ContiguousFrameHandle;
use net_types::{DeviceProvider, LinkStatus, MacAddress};
use virtio_block_server::validate_block_request;
use virtio_gpu_server::{VirtioGpuDriver, VirtioGpuFormat};
use virtio_input_server::{
    ev_type, rel_code, FocusHandle, InputEvent, VirtioInputDriver, VirtioInputEvent,
};
use virtio_net_server::VirtioNetDriver;

#[test]
fn test_end_to_end_virtio_reference_platform_suite() {
    // 1. VirtIO-Block Driver Specification Validation
    let handle = Handle::from_parts(1, 1);
    let dma_handle = ContiguousFrameHandle::from_parts(handle, 0x1000_0000, 4096);
    assert!(validate_block_request(0, 8, 2048, &dma_handle).is_ok());

    // 2. VirtIO-Net Driver Specification Validation
    let mac = MacAddress::new([0x52, 0x54, 0x00, 0x99, 0x88, 0x77]);
    let net_driver = VirtioNetDriver::new(mac);
    assert_eq!(net_driver.mac_address(), mac);
    assert_eq!(net_driver.link_status(), LinkStatus::Up);

    // 3. VirtIO-GPU Display Driver Specification Validation
    let gpu_driver = VirtioGpuDriver::new(1024, 768);
    let create_cmd = gpu_driver.build_resource_create_2d();
    assert_eq!(create_cmd.width, 1024);
    assert_eq!(create_cmd.height, 768);
    assert_eq!(create_cmd.format, VirtioGpuFormat::B8G8R8A8Unorm as u32);

    let mut framebuf = vec![0u8; 1024 * 768 * 4];
    assert!(gpu_driver.draw_test_pattern(&mut framebuf).is_ok());
    // Verify first pixel red channel
    assert_eq!(framebuf[2], 255);

    // 4. VirtIO-Input Keyboard & Mouse Driver Specification Validation
    let mut input_driver = VirtioInputDriver::new();
    let focused_task = GaxObjectId::from_bytes([10; 16]);
    let unfocused_task = GaxObjectId::from_bytes([20; 16]);
    let active_win = GaxObjectId::from_bytes([30; 16]);

    input_driver.set_focus(FocusHandle::new(focused_task, active_win));

    let key_event = VirtioInputEvent {
        event_type: ev_type::EV_KEY,
        code: 28, // KEY_ENTER
        value: 1,  // Press
    };

    // Focused process gets input event
    let decoded = input_driver
        .decode_event(&key_event, focused_task)
        .unwrap();
    assert_eq!(
        decoded,
        InputEvent::Keyboard {
            key_code: 28,
            is_pressed: true
        }
    );

    // Unfocused background process gets blocked (zero keylogging)
    assert!(input_driver
        .decode_event(&key_event, unfocused_task)
        .is_none());

    let pointer_event = VirtioInputEvent {
        event_type: ev_type::EV_REL,
        code: rel_code::REL_Y,
        value: 42,
    };
    let decoded_move = input_driver
        .decode_event(&pointer_event, focused_task)
        .unwrap();
    assert_eq!(decoded_move, InputEvent::PointerMove { dx: 0, dy: 42 });
}
