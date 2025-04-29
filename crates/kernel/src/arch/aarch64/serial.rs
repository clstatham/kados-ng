use core::fmt::{self, Write};

use arm_pl011::Pl011Uart;
use spin::{Once, mutex::SpinMutex};

use crate::arch::driver::{Driver, register_driver};

const PL011_BASE: *mut u8 = 0x0900_0000 as *mut u8; // Base address of PL011 UART

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

pub struct UartDriver;

impl Driver for UartDriver {
    fn name(&self) -> &'static str {
        "PL011 UART"
    }

    unsafe fn init(&self) -> Result<(), &'static str> {
        let mut uart = Pl011Uart::new(PL011_BASE);
        uart.init();
        UART.call_once(|| SpinMutex::new(Uart(uart)));
        Ok(())
    }
}

pub fn register() {
    register_driver(&UartDriver, "PLO11 UART", None).expect("Failed to register UART driver");
}
