use core::fmt::{self, Write};

use arm_pl011::Pl011Uart;
use spin::{Once, mutex::SpinMutex};

const PL011_BASE: *mut u8 = 0xFE201000 as *mut u8; // RPi4 PL011 UART base address

pub struct Uart(Pl011Uart);

impl Write for Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.0.putchar(byte);
        }
        Ok(())
    }
}

pub static UART: Once<SpinMutex<Uart>> = Once::new();

pub fn write_fmt(args: fmt::Arguments) {
    if let Some(uart) = UART.get() {
        uart.lock().write_fmt(args).ok();
    }
}

pub fn init() {
    let mut uart = Pl011Uart::new(PL011_BASE);
    uart.init();
    UART.call_once(|| SpinMutex::new(Uart(uart)));
}
