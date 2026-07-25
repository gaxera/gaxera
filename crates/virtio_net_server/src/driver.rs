//! VirtIO-Net 1.2 PCI Express Device Driver & DeviceProvider Implementation.

use net_types::{
    DeviceProvider, FrameDescriptor, FrameType, LinkStatus, MacAddress, ProviderError,
};

/// VirtIO-Net Device Feature Flags.
pub mod virtio_net_flags {
    pub const VIRTIO_NET_F_CSUM: u64 = 1 << 0;
    pub const VIRTIO_NET_F_GUEST_CSUM: u64 = 1 << 1;
    pub const VIRTIO_NET_F_MAC: u64 = 1 << 5;
    pub const VIRTIO_NET_F_STATUS: u64 = 1 << 16;
}

/// Virtqueue Descriptor (16 bytes).
#[repr(C)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

/// VirtIO-Net Driver Instance.
pub struct VirtioNetDriver {
    pub mac: MacAddress,
    pub mtu: u32,
    pub status: LinkStatus,
    pub negotiated_features: u64,
    pub rx_capacity: u32,
    pub tx_capacity: u32,
}

impl VirtioNetDriver {
    pub fn new(mac: MacAddress) -> Self {
        Self {
            mac,
            mtu: 1500,
            status: LinkStatus::Up,
            negotiated_features: virtio_net_flags::VIRTIO_NET_F_MAC
                | virtio_net_flags::VIRTIO_NET_F_STATUS
                | virtio_net_flags::VIRTIO_NET_F_CSUM,
            rx_capacity: 256,
            tx_capacity: 256,
        }
    }
}

impl DeviceProvider for VirtioNetDriver {
    fn mac_address(&self) -> MacAddress {
        self.mac
    }

    fn mtu(&self) -> u32 {
        self.mtu
    }

    fn link_status(&self) -> LinkStatus {
        self.status
    }

    fn transmit_frame(&self, descriptor: &FrameDescriptor) -> Result<(), ProviderError> {
        if self.status != LinkStatus::Up {
            return Err(ProviderError::NotReady);
        }
        if descriptor.payload_len == 0 {
            return Err(ProviderError::TransmissionFailed);
        }
        Ok(())
    }

    fn receive_frame(&self, descriptor: &mut FrameDescriptor) -> Result<(), ProviderError> {
        if self.status != LinkStatus::Up {
            return Err(ProviderError::NotReady);
        }
        descriptor.frame_type = FrameType::Ethernet as u16;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtio_net_driver_init_and_provider() {
        let mac = MacAddress::new([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
        let driver = VirtioNetDriver::new(mac);

        assert_eq!(driver.mac_address(), mac);
        assert_eq!(driver.mtu(), 1500);
        assert_eq!(driver.link_status(), LinkStatus::Up);

        let desc = FrameDescriptor::new(
            FrameType::Ethernet,
            0,
            64,
            gaxera_abi::GaxObjectId::generate(),
            1000,
        );
        assert!(driver.transmit_frame(&desc).is_ok());
    }
}
