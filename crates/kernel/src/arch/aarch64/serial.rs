use core::fmt::{self, Write};

use spin::{Mutex, MutexGuard};

/* -------- base addresses ------------------------------------------------ */

/// The base address for the BCM2711 peripherals.
pub const PERIPHERAL_BASE: usize = 0xFE00_0000; // BCM2711 peripheral window
/// The base address for the GPIO registers.
pub const GPIO_BASE: usize = PERIPHERAL_BASE + 0x20_0000;
/// The base address for the clock manager registers.
pub const CM_BASE: usize = PERIPHERAL_BASE + 0x10_0000; // clock manager
/// The base address for the UART0 registers.
pub const UART0_BASE: usize = PERIPHERAL_BASE + 0x20_1000;

/* -------- GPIO registers we need --------------------------------------- */

const GPFSEL1: *mut u32 = (GPIO_BASE + 0x04) as *mut u32;
const GPPUD: *mut u32 = (GPIO_BASE + 0x94) as *mut u32;
const GPPUDCLK0: *mut u32 = (GPIO_BASE + 0x98) as *mut u32;

/* -------- CM UART clock (GPCLK UART) ----------------------------------- */

const CM_UARTCTL: *mut u32 = (CM_BASE + 0x1F68) as *mut u32; // CTL
const CM_UARTDIV: *mut u32 = (CM_BASE + 0x1F6C) as *mut u32; // DIV

/* -------- PL011 register block ----------------------------------------- */

const DR: *mut u32 = (UART0_BASE + 0x00) as *mut u32;
const FR: *mut u32 = (UART0_BASE + 0x18) as *mut u32;
const IBRD: *mut u32 = (UART0_BASE + 0x24) as *mut u32;
const FBRD: *mut u32 = (UART0_BASE + 0x28) as *mut u32;
const LCRH: *mut u32 = (UART0_BASE + 0x2C) as *mut u32;
const CR: *mut u32 = (UART0_BASE + 0x30) as *mut u32;
const ICR: *mut u32 = (UART0_BASE + 0x44) as *mut u32;

/// An instance of the GPIO UART driver.
pub struct GpioUart {
    _private: (),
}

impl GpioUart {
    /// Initializes the GPIO UART driver.
    pub fn init(&mut self) {
        use core::ptr::{read_volatile, write_volatile};
        // thanks, chatGPT
        unsafe {
            /* 0 ─── Enable the 48‑MHz UART clock (GPCLK UART) */
            //
            //  DIV = 3  → 48 MHz   (PLLD: 540 MHz / 3 / 5 = 36 MHz; CM mixes 3 & 0 settings,
            //                       but 48 MHz is what the Pi firmware & Linux use)
            //  SRC = 6  → PLLD
            //  ENAB bit must be set last.
            //
            write_volatile(CM_UARTDIV, 3); // DIVI = 3
            write_volatile(CM_UARTCTL, 0x0000_2160); // ENAB | BUSY | SRC=PLLD | KILL=0
            for _ in 0..150 {
                core::arch::asm!("nop");
            } // ~150 core cycles

            /* 1 ─── Pin‑mux: GPIO 14/15 to ALT0 (TXD0/RXD0) */
            let mut sel = read_volatile(GPFSEL1);
            sel &= !((0b111 << 12) | (0b111 << 15)); // clear both fields
            sel |= (0b100 << 12) | (0b100 << 15); // ALT0 = 0b100
            write_volatile(GPFSEL1, sel);
            // disable pulls
            write_volatile(GPPUD, 0);
            for _ in 0..150 {
                core::arch::asm!("nop");
            }
            write_volatile(GPPUDCLK0, (1 << 14) | (1 << 15));
            for _ in 0..150 {
                core::arch::asm!("nop");
            }
            write_volatile(GPPUDCLK0, 0);

            /* 2 ─── Disable UART, wait until BUSY clears */
            write_volatile(CR, 0);
            while read_volatile(FR) & (1 << 3) != 0 {} // BUSY

            /* 3 ─── Clear pending interrupts */
            write_volatile(ICR, 0x7FF);

            // /* 4 ─── Baud: 921600 bps */
            write_volatile(IBRD, 3);
            write_volatile(FBRD, 16);

            /* 5 ─── 8 data bits, FIFO enabled */
            write_volatile(LCRH, (1 << 4) | (3 << 5)); // FEN | WLEN=0b11 (8 bits)

            /* 6 ─── Enable RX, TX and the UART */
            write_volatile(CR, (1 << 9) | (1 << 8) | 1); // RXE | TXE | UARTEN
            core::arch::asm!("dsb sy; isb");
        }
    }

    /// Writes a character to the UART.
    #[inline]
    pub fn putchar(&mut self, c: u8) {
        unsafe {
            loop {
                let fr = FR.read_volatile();
                if fr & (1 << 5) != 0 {
                    core::arch::asm!("nop");
                } else {
                    break;
                }
            }
            DR.write_volatile(u32::from(c));
        }
    }

    /// Waits for a character to be available and reads it from the UART.
    #[inline]
    pub fn getchar(&mut self) -> u8 {
        unsafe {
            loop {
                let fr = FR.read_volatile();
                if fr & 0x10 != 0 {
                    core::arch::asm!("nop");
                } else {
                    break;
                }
            }
            DR.read_volatile() as u8
        }
    }

    /// Tries to read a character from the UART without blocking.
    ///
    /// Returns `Some(byte)` if a character is available, or `None` if not.
    #[inline]
    pub fn try_getchar(&mut self) -> Option<u8> {
        unsafe {
            let fr = FR.read_volatile();
            if fr & 0x10 != 0 {
                None
            } else {
                Some(DR.read_volatile() as u8)
            }
        }
    }
}

static UART: Mutex<GpioUart> = Mutex::new(GpioUart { _private: () });

impl Write for GpioUart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.putchar(b'\r');
            }
            self.putchar(b);
        }
        Ok(())
    }
}

/// Locks the UART for exclusive access.
pub fn lock_uart<'a>() -> MutexGuard<'a, GpioUart> {
    UART.lock()
}

/// Writes a formatted string to the UART.
pub fn write_fmt(args: fmt::Arguments) {
    UART.lock().write_fmt(args).ok();
}

/// Initializes the GPIO UART driver.
pub fn init() {
    UART.lock().init();
}
