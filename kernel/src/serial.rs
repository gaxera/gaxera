use core::arch::asm;
use core::fmt::Write;

pub static COM1: SerialPort = SerialPort::new(0x3F8);

pub struct SerialPort {
    port: u16,
}

impl SerialPort {
    pub const fn new(port: u16) -> Self {
        Self { port }
    }

    /// Initialize the 16550 UART serial port configuration.
    ///
    /// # Safety
    /// This function performs raw port I/O operations and must only be called
    /// when the hardware is ready to configure COM1 interface registers.
    pub unsafe fn init(&self) {
        unsafe {
            // Disable all interrupts
            outb(self.port + 1, 0x00);
            // Enable DLAB (set baud rate divisor)
            outb(self.port + 3, 0x80);
            // Set divisor to 3 (lo byte) -> 38400 baud
            outb(self.port, 0x03);
            // Set divisor to 0 (hi byte)
            outb(self.port + 1, 0x00);
            // 8 bits, no parity, one stop bit
            outb(self.port + 3, 0x03);
            // Enable FIFO, clear them, with 14-byte threshold
            outb(self.port + 2, 0xC7);
            // RTS/DSR set
            outb(self.port + 4, 0x0B);
        }
    }

    /// Write a raw byte to the serial transmit line.
    pub fn write_byte(&self, byte: u8) {
        // SAFETY: The boot environment has a 16550-compatible UART at COM1.
        // We intentionally do not poll LSR here: early emulator environments
        // can otherwise wedge before diagnostics are emitted.
        unsafe {
            outb(self.port, byte);
        }
    }

    /// Write a string slice to the serial output.
    pub fn write_str(&self, s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
    }
}

pub fn halt() -> ! {
    loop {
        // SAFETY: Limine enters with interrupts disabled. Halting is therefore
        // a terminal low-power state until an external reset, which is exactly
        // the intended post-proof behavior before an interrupt subsystem exists.
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}

// Low-level port I/O abstractions using inline assembly.
#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") val,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Helper writer structure to implement core::fmt::Write without global lock references.
pub struct SerialWriter;

impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        COM1.write_str(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::print!("{}\n", format_args!($($arg)*))
    };
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    let mut writer = SerialWriter;
    writer.write_fmt(args).unwrap();
}
