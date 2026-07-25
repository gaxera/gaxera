//! VirtIO-GPU 2D Display Scanout Driver & Framebuffer Management.

use core::fmt;

/// VirtIO-GPU Command Error Enums.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum GpuError {
    BufferTooSmall = 1,
    InvalidResource = 2,
    ScanoutFailed = 3,
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// VirtIO-GPU Command Types.
pub mod gpu_cmd {
    pub const VIRTIO_GPU_CMD_GET_DISPLAY_INFO: u32 = 0x0100;
    pub const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
    pub const VIRTIO_GPU_CMD_RESOURCE_UNREF: u32 = 0x0102;
    pub const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x0103;
    pub const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x0104;
    pub const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
    pub const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
    pub const VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;
}

/// VirtIO-GPU Response Types.
pub mod gpu_resp {
    pub const VIRTIO_GPU_RESP_OK_NODATA: u32 = 0x1100;
    pub const VIRTIO_GPU_RESP_OK_DISPLAY_INFO: u32 = 0x1101;
    pub const VIRTIO_GPU_RESP_ERR_UNSPEC: u32 = 0x1200;
}

/// VirtIO-GPU Pixel Formats.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum VirtioGpuFormat {
    B8G8R8A8Unorm = 1,
    B8G8R8X8Unorm = 2,
    A8R8G8B8Unorm = 3,
    R8G8B8A8Unorm = 67,
}

/// 2D Bounding Rectangle.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct VirtioGpuRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Common Control Header (24 bytes).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct VirtioGpuCtrlHeader {
    pub type_hdr: u32,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub ring_idx: u8,
    pub padding: [u8; 3],
}

/// Command: Resource Create 2D.
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct CmdResourceCreate2d {
    pub hdr: VirtioGpuCtrlHeader,
    pub resource_id: u32,
    pub format: u32,
    pub width: u32,
    pub height: u32,
}

/// Command: Resource Attach Backing.
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct CmdResourceAttachBacking {
    pub hdr: VirtioGpuCtrlHeader,
    pub resource_id: u32,
    pub nr_entries: u32,
}

/// Command: Transfer To Host 2D.
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct CmdTransferToHost2d {
    pub hdr: VirtioGpuCtrlHeader,
    pub r: VirtioGpuRect,
    pub offset: u64,
    pub resource_id: u32,
    pub padding: u32,
}

/// Command: Set Scanout.
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct CmdSetScanout {
    pub hdr: VirtioGpuCtrlHeader,
    pub r: VirtioGpuRect,
    pub scanout_id: u32,
    pub resource_id: u32,
}

/// Command: Resource Flush.
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct CmdResourceFlush {
    pub hdr: VirtioGpuCtrlHeader,
    pub r: VirtioGpuRect,
    pub resource_id: u32,
    pub padding: u32,
}

/// Ring-3 VirtIO-GPU Driver State Machine.
pub struct VirtioGpuDriver {
    pub width: u32,
    pub height: u32,
    pub format: VirtioGpuFormat,
    pub active_resource_id: u32,
    pub is_scanout_active: bool,
}

impl VirtioGpuDriver {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            format: VirtioGpuFormat::B8G8R8A8Unorm,
            active_resource_id: 1,
            is_scanout_active: false,
        }
    }

    /// Build Create 2D Resource Command.
    pub fn build_resource_create_2d(&self) -> CmdResourceCreate2d {
        CmdResourceCreate2d {
            hdr: VirtioGpuCtrlHeader {
                type_hdr: gpu_cmd::VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                ring_idx: 0,
                padding: [0; 3],
            },
            resource_id: self.active_resource_id,
            format: self.format as u32,
            width: self.width,
            height: self.height,
        }
    }

    /// Build Set Scanout Command.
    pub fn build_set_scanout(&self) -> CmdSetScanout {
        CmdSetScanout {
            hdr: VirtioGpuCtrlHeader {
                type_hdr: gpu_cmd::VIRTIO_GPU_CMD_SET_SCANOUT,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                ring_idx: 0,
                padding: [0; 3],
            },
            r: VirtioGpuRect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            },
            scanout_id: 0,
            resource_id: self.active_resource_id,
        }
    }

    /// Draw a high-contrast test pattern (RGBA color bars) into a raw pixel buffer.
    pub fn draw_test_pattern(&self, buf: &mut [u8]) -> Result<(), GpuError> {
        let stride = (self.width * 4) as usize;
        let expected_len = stride * (self.height as usize);
        if buf.len() < expected_len {
            return Err(GpuError::BufferTooSmall);
        }

        for y in 0..self.height {
            for x in 0..self.width {
                let idx = ((y * self.width + x) * 4) as usize;
                let color_idx = (x * 8 / self.width) as u8;
                let (r, g, b) = match color_idx {
                    0 => (255, 0, 0),     // Red
                    1 => (0, 255, 0),     // Green
                    2 => (0, 0, 255),     // Blue
                    3 => (255, 255, 0),   // Yellow
                    4 => (0, 255, 255),   // Cyan
                    5 => (255, 0, 255),   // Magenta
                    6 => (255, 255, 255), // White
                    _ => (30, 30, 30),    // Dark Gray
                };
                buf[idx] = b; // Blue channel
                buf[idx + 1] = g; // Green channel
                buf[idx + 2] = r; // Red channel
                buf[idx + 3] = 255; // Alpha
            }
        }
        Ok(())
    }
}
