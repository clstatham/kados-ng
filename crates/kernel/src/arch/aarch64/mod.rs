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
            // Self::invalidate_all();
            // asm!(
            //     "mov x9, sp
            //     ldr x10, [x9]
            //     "
            // );
            // log::debug!("Woohoo!");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_aarch64_consts() {
        assert_eq!(AArch64::PAGE_SIZE, 4096);
        assert_eq!(AArch64::PAGE_OFFSET_MASK, 0xFFF);
        assert_eq!(AArch64::PAGE_ADDR_SHIFT, 48);
        assert_eq!(AArch64::PAGE_ADDR_SIZE, 0x0001_0000_0000_0000);
        assert_eq!(AArch64::PAGE_ADDR_MASK, 0x0000_FFFF_FFFF_F000);
        assert_eq!(AArch64::PAGE_ENTRY_SIZE, 8);
        assert_eq!(AArch64::PAGE_ENTRIES, 512);
        assert_eq!(AArch64::PAGE_ENTRY_MASK, 0x1FF);
        // assert_eq!(AArch64::PAGE_NEG_MASK, 0xFFFF_0000_0000_0000);

        assert_eq!(AArch64::PAGE_ENTRY_ADDR_SIZE, 0x0000_0100_0000_0000);
        assert_eq!(AArch64::PAGE_ENTRY_ADDR_MASK, 0x0000_00FF_FFFF_FFFF);
        assert_eq!(AArch64::PAGE_ENTRY_FLAGS_MASK, 0xFFF0_0000_0000_0FFF);

        // assert_eq!(AArch64::PHYS_OFFSET, 0xFFFF_8000_0000_0000);
    }
}
