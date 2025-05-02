use core::arch::asm;

use crate::mem::units::{PhysAddr, VirtAddr};

use super::ArchTrait;

pub mod mem;
pub mod random;
pub mod serial;
pub mod time;

pub struct AArch64;

impl ArchTrait for AArch64 {
    const PAGE_SHIFT: usize = 12;

    const PAGE_ENTRY_SHIFT: usize = 9;

    const PAGE_LEVELS: usize = 4;

    const PAGE_ENTRY_ADDR_WIDTH: usize = 40;

    const PAGE_FLAG_PAGE_DEFAULTS: usize = Self::PAGE_FLAG_PRESENT | 1 << 1 | 1 << 10;

    const PAGE_FLAG_TABLE_DEFAULTS: usize =
        Self::PAGE_FLAG_PRESENT | Self::PAGE_FLAG_READWRITE | 1 << 1 | 1 << 10;

    const PAGE_FLAG_PRESENT: usize = 1 << 0;

    const PAGE_FLAG_READONLY: usize = 1 << 7;

    const PAGE_FLAG_READWRITE: usize = 0;

    const PAGE_FLAG_USER: usize = 1 << 6;

    const PAGE_FLAG_EXECUTABLE: usize = 0;

    const PAGE_FLAG_NON_EXECUTABLE: usize = 0b11 << 53;

    const PAGE_FLAG_GLOBAL: usize = 0;

    const PAGE_FLAG_NON_GLOBAL: usize = 1 << 11;

    unsafe fn init_mem() {
        unsafe {
            mem::init();
        }
    }

    unsafe fn init_interrupts() {
        unsafe {
            super::vectors::init();
        }
    }

    unsafe fn enable_interrupts() {
        unsafe {
            asm!(
                "
                msr daifclr, #0b1111
                "
            )
        }
    }

    unsafe fn disable_interrupts() {
        unsafe { asm!("msr daifset, #0b1111") }
    }

    unsafe fn interrupts_enabled() -> bool {
        todo!()
    }

    unsafe fn invalidate_page(addr: VirtAddr) {
        unsafe {
            asm!("
            dsb ishst
            tlbi vaae1is, {}
            dsb ish
            isb
        ", in(reg) (addr.value() >> Self::PAGE_SHIFT));
        }
    }

    unsafe fn invalidate_all() {
        unsafe {
            asm!("dsb ishst");
            asm!("tlbi vmalle1is");
            asm!("dsb ish");
            asm!("isb");
        }
    }

    unsafe fn current_page_table() -> PhysAddr {
        let addr: usize;
        unsafe {
            asm!("mrs {}, ttbr1_el1", out(reg) addr);
        }
        PhysAddr::new_canonical(addr)
    }

    unsafe fn set_current_page_table(addr: PhysAddr) {
        unsafe {
            asm!("
            dsb ishst
            msr ttbr1_el1, {}
            dsb ish
            isb", 
            in(reg) addr.value());
        }
    }

    unsafe fn set_stack_pointer(sp: VirtAddr, next_fn: extern "C" fn() -> !) -> ! {
        unsafe {
            core::arch::asm!(
                "msr SPSel, #1",
                "mov sp, {}",
                "mov fp, xzr",
                "br {}",
                in(reg) sp.value(),
                in(reg) next_fn, options(noreturn)
            );
        }
    }

    fn exit_qemu(code: u32) -> ! {
        use qemu_exit::QEMUExit;
        qemu_exit::AArch64::new().exit(code)
    }

    fn hcf() -> ! {
        loop {
            unsafe {
                asm!("wfe");
            }
        }
    }
}
