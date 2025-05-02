pub mod driver;
pub mod vectors;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

#[cfg(target_arch = "aarch64")]
pub use self::aarch64::AArch64 as Arch;
#[cfg(target_arch = "aarch64")]
pub use self::aarch64::*;

use crate::mem::units::{PhysAddr, VirtAddr};

pub trait ArchTrait {
    const PAGE_SHIFT: usize;
    const PAGE_ENTRY_SHIFT: usize;
    const PAGE_LEVELS: usize;
    const PAGE_ENTRY_ADDR_WIDTH: usize;
    const PAGE_ENTRY_ADDR_SHIFT: usize = Self::PAGE_SHIFT;

    const PAGE_FLAG_PAGE_DEFAULTS: usize;
    const PAGE_FLAG_TABLE_DEFAULTS: usize;
    const PAGE_FLAG_PRESENT: usize;
    const PAGE_FLAG_READONLY: usize;
    const PAGE_FLAG_READWRITE: usize;
    const PAGE_FLAG_USER: usize;
    const PAGE_FLAG_EXECUTABLE: usize;
    const PAGE_FLAG_NON_EXECUTABLE: usize;
    const PAGE_FLAG_GLOBAL: usize;
    const PAGE_FLAG_NON_GLOBAL: usize;

    const PAGE_SIZE: usize = 1 << Self::PAGE_SHIFT;
    const PAGE_OFFSET_MASK: usize = Self::PAGE_SIZE - 1;

    const PAGE_ENTRIES: usize = 1 << Self::PAGE_ENTRY_SHIFT;
    const PAGE_ENTRY_MASK: usize = Self::PAGE_ENTRIES - 1;
    const PAGE_ENTRY_SIZE: usize = 1 << (Self::PAGE_SHIFT - Self::PAGE_ENTRY_SHIFT);
    const PAGE_ADDR_SHIFT: usize = Self::PAGE_LEVELS * Self::PAGE_ENTRY_SHIFT + Self::PAGE_SHIFT;
    const PAGE_ADDR_SIZE: usize = 1 << Self::PAGE_ADDR_SHIFT;
    const PAGE_ADDR_MASK: usize = Self::PAGE_ADDR_SIZE - Self::PAGE_SIZE;
    const PAGE_ENTRY_ADDR_SIZE: usize = 1 << Self::PAGE_ENTRY_ADDR_WIDTH;
    const PAGE_ENTRY_ADDR_MASK: usize = Self::PAGE_ENTRY_ADDR_SIZE - 1;
    const PAGE_ENTRY_FLAGS_MASK: usize =
        !(Self::PAGE_ENTRY_ADDR_MASK << Self::PAGE_ENTRY_ADDR_SHIFT);

    unsafe fn init_mem();
    unsafe fn init_interrupts();
    unsafe fn enable_interrupts();
    unsafe fn disable_interrupts();
    unsafe fn set_interrupts_enabled(enable: bool) {
        unsafe {
            if enable {
                Self::enable_interrupts();
            } else {
                Self::disable_interrupts();
            }
        }
    }
    unsafe fn interrupts_enabled() -> bool;

    unsafe fn invalidate_page(addr: VirtAddr);
    unsafe fn invalidate_all();

    unsafe fn current_page_table() -> PhysAddr;
    unsafe fn set_current_page_table(addr: PhysAddr);

    unsafe fn set_stack_pointer(sp: VirtAddr, next_fn: extern "C" fn() -> !) -> !;

    fn exit_qemu(code: u32) -> !;
    fn hcf() -> !;
}
