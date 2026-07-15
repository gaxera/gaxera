use core::fmt;
use core::ptr::write_volatile;
use limine::framebuffer::{FRAMEBUFFER_RGB, Framebuffer as LimineFramebuffer};

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
    /// Create a bounded wrapper for the framebuffer Limine mapped at handoff.
    ///
    /// # Safety
    /// `framebuffer` must remain mapped and writable for `height * pitch` bytes.
    /// Limine guarantees this while its initial page tables remain active.
    pub unsafe fn from_limine(framebuffer: &LimineFramebuffer) -> Result<Self, FramebufferError> {
        if framebuffer.address().is_null()
            || framebuffer.width == 0
            || framebuffer.height == 0
            || framebuffer.bpp != 32
            || framebuffer.memory_model != FRAMEBUFFER_RGB
            || framebuffer.red_mask_size != 8
            || framebuffer.green_mask_size != 8
            || framebuffer.blue_mask_size != 8
            || !framebuffer.red_mask_shift.is_multiple_of(8)
            || !framebuffer.green_mask_shift.is_multiple_of(8)
            || !framebuffer.blue_mask_shift.is_multiple_of(8)
            || framebuffer.red_mask_shift >= 32
            || framebuffer.green_mask_shift >= 32
            || framebuffer.blue_mask_shift >= 32
        {
            return Err(FramebufferError::UnsupportedPixelFormat);
        }

        let row_bytes = framebuffer
            .width
            .checked_mul(4)
            .ok_or(FramebufferError::InvalidLayout)?;
        let size = framebuffer
            .height
            .checked_mul(framebuffer.pitch)
            .ok_or(FramebufferError::InvalidLayout)?;
        if framebuffer.pitch < row_bytes || usize::try_from(size).is_err() {
            return Err(FramebufferError::InvalidLayout);
        }

        Ok(Self {
            addr: framebuffer.address().cast(),
            width: framebuffer.width,
            height: framebuffer.height,
            pitch: framebuffer.pitch,
            red_byte: usize::from(framebuffer.red_mask_shift / 8),
            green_byte: usize::from(framebuffer.green_mask_shift / 8),
            blue_byte: usize::from(framebuffer.blue_mask_shift / 8),
        })
    }

    /// Write an RGB pixel, returning without writing outside the framebuffer.
    #[inline]
    pub fn draw_pixel(&self, x: u64, y: u64, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height {
            return;
        }

        let offset = (y * self.pitch + x * 4) as usize;
        // SAFETY: `from_limine` validates the row layout and this method bounds
        // x/y, so every byte written is within Limine's mapped framebuffer.
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
