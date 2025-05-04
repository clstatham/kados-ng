pub use crate::arch::serial::*;

use core::fmt::{self};

#[inline(always)]
pub fn _print(args: fmt::Arguments) {
    crate::arch::serial::write_fmt(args);
}
