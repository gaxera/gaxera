#![no_std]

extern crate alloc;

use libgaxera::driver::ContiguousFrameHandle;
use libgaxera::virtio::VirtioNetHeader;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PacketState {
    Free,
    RxReady,
    RxFilled,
    TxPending,
    Delivered,
}

pub struct PacketBuffer {
    pub buffer_id: u16,
    pub header: VirtioNetHeader,
    pub dma_handle: ContiguousFrameHandle,
    pub payload_len: u16,
    pub state: PacketState,
}

impl PacketBuffer {
    pub fn new(buffer_id: u16, dma_handle: ContiguousFrameHandle) -> Self {
        Self {
            buffer_id,
            header: VirtioNetHeader::default(),
            dma_handle,
            payload_len: 0,
            state: PacketState::Free,
        }
    }
}

pub struct VirtioNetServer {
    pub mac_address: [u8; 6],
}

impl VirtioNetServer {
    pub fn new(mac_address: [u8; 6]) -> Self {
        Self { mac_address }
    }

    pub fn mac_address(&self) -> [u8; 6] {
        self.mac_address
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaxera_abi::Handle;

    #[test]
    fn packet_buffer_lifecycle_states() {
        let dma_handle =
            ContiguousFrameHandle::from_parts(Handle::from_parts(20, 1), 0x6000_0000, 2048);
        let mut pkt = PacketBuffer::new(0, dma_handle);

        assert_eq!(pkt.state, PacketState::Free);
        pkt.state = PacketState::RxReady;
        assert_eq!(pkt.state, PacketState::RxReady);
        pkt.state = PacketState::RxFilled;
        assert_eq!(pkt.state, PacketState::RxFilled);
        pkt.state = PacketState::Delivered;
        assert_eq!(pkt.state, PacketState::Delivered);
    }
}
