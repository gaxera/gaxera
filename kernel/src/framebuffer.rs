use core::fmt;
use core::ptr::write_volatile;

use crate::memory::boot::FramebufferInfo;

pub struct Framebuffer {
    addr: *mut u8,
    width: u64,
    height: u64,
    pitch: u64,
    red_byte: usize,
    green_byte: usize,
    blue_byte: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FramebufferError {
    InvalidLayout,
    UnsupportedPixelFormat,
}

impl fmt::Display for FramebufferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLayout => f.write_str("invalid dimensions, pitch, or base address"),
            Self::UnsupportedPixelFormat => {
                f.write_str("only 32-bit RGB framebuffers are supported")
            }
        }
    }
}

impl Framebuffer {
    /// Create a bounded wrapper for a Gaxera-owned framebuffer description.
    ///
    /// # Safety
    /// `virtual_address` must map `info.size` writable bytes using the exact
    /// framebuffer physical range captured in `BootContext`.
    pub unsafe fn from_boot_context(
        info: FramebufferInfo,
        virtual_address: u64,
    ) -> Result<Self, FramebufferError> {
        if virtual_address == 0 || usize::try_from(info.size).is_err() {
            return Err(FramebufferError::InvalidLayout);
        }

        Ok(Self {
            addr: virtual_address as *mut u8,
            width: info.width,
            height: info.height,
            pitch: info.pitch,
            red_byte: info.red_byte,
            green_byte: info.green_byte,
            blue_byte: info.blue_byte,
        })
    }

    /// Write an RGB pixel, returning without writing outside the framebuffer.
    #[inline]
    pub fn draw_pixel(&self, x: u64, y: u64, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height {
            return;
        }

        let offset = (y * self.pitch + x * 4) as usize;
        // SAFETY: `from_boot_context` accepts only metadata captured and
        // validated at handoff, and this method bounds x/y within that layout.
        unsafe {
            write_volatile(self.addr.add(offset + self.red_byte), r);
            write_volatile(self.addr.add(offset + self.green_byte), g);
            write_volatile(self.addr.add(offset + self.blue_byte), b);
        }
    }

    /// Draw a smooth RGB gradient test pattern across the screen.
    pub fn draw_test_pattern(&self) {
        for y in 0..self.height {
            for x in 0..self.width {
                let r = (x * 255 / self.width) as u8;
                let g = (y * 255 / self.height) as u8;
                let b = ((x + y) * 255 / (self.width + self.height)) as u8;
                self.draw_pixel(x, y, r, g, b);
            }
        }
    }
}
