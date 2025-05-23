use fdt::Fdt;

use crate::syscall::errno::Errno;

use super::Architecture;

pub trait Driver: 'static {
    type Arch: Architecture;

    const CONST_DEFAULT: Self;

    unsafe fn init(&mut self, fdt: &Fdt) -> Result<(), Errno>;
}
