use fdt::Fdt;

use crate::syscall::errno::Errno;

use super::ArchTrait;

pub trait DriverTrait: 'static {
    type Arch: ArchTrait;
    const CONST_DEFAULT: Self;

    unsafe fn init(&mut self, fdt: &Fdt) -> Result<(), Errno>;
}
