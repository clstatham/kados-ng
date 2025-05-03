use uart_16550::SerialPort;

use crate::sync::IrqMutex;

pub const SERIAL0_IOPORT: u16 = 0x3f8;

pub static SERIAL0: IrqMutex<SerialPort> =
    unsafe { IrqMutex::new(SerialPort::new(SERIAL0_IOPORT)) };

pub fn init() {
    SERIAL0.lock().init();
}

#[doc(hidden)]
#[allow(unreachable_code)]
pub fn write_fmt(args: ::core::fmt::Arguments) {
    use core::fmt::Write;
    // #[cfg(debug_assertions)]
    x86_64::instructions::interrupts::without_interrupts(|| {
        SERIAL0
            .lock()
            .write_fmt(args)
            .expect("Printing to serial failed");
    });
}
