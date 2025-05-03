pub use crate::arch::serial::*;

use core::fmt::{self};

#[inline(always)]
pub fn _print(args: fmt::Arguments) {
    crate::arch::serial::write_fmt(args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        let _ = $crate::serial::_print(format_args!($($arg)*));
    });
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
}
