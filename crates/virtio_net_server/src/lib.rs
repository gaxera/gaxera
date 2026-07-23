#![no_std]

extern crate alloc;

use gaxera_abi::net::NetError;
use libgaxera::driver::ContiguousFrameHandle;
use libgaxera::virtio::VirtioNetHeader;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PacketState {
    Free,
    QueuedToRx,
    QueuedToTx,
    InUseByUser,
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

    pub fn queue_rx(&mut self) -> Result<(), NetError> {
        if self.state != PacketState::Free {
            return Err(NetError::InvalidBufferId);
        }
        self.state = PacketState::QueuedToRx;
        Ok(())
    }

    pub fn queue_tx(&mut self) -> Result<(), NetError> {
        if self.state != PacketState::Free && self.state != PacketState::InUseByUser {
            return Err(NetError::InvalidBufferId);
        }
        self.state = PacketState::QueuedToTx;
        Ok(())
    }

    pub fn deliver_to_user(&mut self) -> Result<(), NetError> {
        if self.state != PacketState::QueuedToRx {
            return Err(NetError::InvalidBufferId);
        }
        self.state = PacketState::InUseByUser;
        Ok(())
    }

    pub fn release(&mut self) -> Result<(), NetError> {
        if self.state == PacketState::Free {
            return Err(NetError::InvalidBufferId);
        }
        self.state = PacketState::Free;
        self.payload_len = 0;
        Ok(())
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
        assert!(pkt.queue_rx().is_ok());
        assert_eq!(pkt.state, PacketState::QueuedToRx);
        assert!(pkt.deliver_to_user().is_ok());
        assert_eq!(pkt.state, PacketState::InUseByUser);
        assert!(pkt.release().is_ok());
        assert_eq!(pkt.state, PacketState::Free);

        // Double release rejection
        assert_eq!(pkt.release(), Err(NetError::InvalidBufferId));
    }
}
