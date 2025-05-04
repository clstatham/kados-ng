use core::{arch::asm, marker::PhantomData, ops::Deref};

use aarch64_cpu::registers::*;

use crate::mem::units::{PhysAddr, VirtAddr};

use super::ArchTrait;

pub mod random;
pub mod serial;
pub mod time;
pub mod vectors;

pub struct AArch64;

impl ArchTrait for AArch64 {
    const PAGE_SHIFT: usize = 12;

    const PAGE_ENTRY_SHIFT: usize = 9;

    const PAGE_LEVELS: usize = 4;

    const PAGE_ENTRY_ADDR_WIDTH: usize = 40;

    const PAGE_FLAG_PAGE_DEFAULTS: usize = Self::PAGE_FLAG_PRESENT | 1 << 1 | 1 << 10;

    const PAGE_FLAG_TABLE_DEFAULTS: usize =
        Self::PAGE_FLAG_PRESENT | Self::PAGE_FLAG_READWRITE | 1 << 1;

    const PAGE_FLAG_PRESENT: usize = 1 << 0;

    const PAGE_FLAG_READONLY: usize = 1 << 7;

    const PAGE_FLAG_READWRITE: usize = 0;

    const PAGE_FLAG_USER: usize = 1 << 6;

    const PAGE_FLAG_EXECUTABLE: usize = 0;

    const PAGE_FLAG_NON_EXECUTABLE: usize = 0b11 << 53;

    const PAGE_FLAG_GLOBAL: usize = 0;

    const PAGE_FLAG_NON_GLOBAL: usize = 1 << 11;

    unsafe fn init_pre_kernel_main() {}

    unsafe fn init_mem() {
        MAIR_EL1.set((0x44 << 8) | 0xff); // NORMAL_UNCACHED_MEMORY, NORMAL_WRITEBACK_MEMORY
    }

    unsafe fn init_post_heap() {}

    unsafe fn init_interrupts() {
        unsafe {
            super::vectors::init();
        }
    }

    #[inline(always)]
    unsafe fn enable_interrupts() {
        unsafe {
            asm!(
                "
                msr daifclr, #0b1111
                "
            )
        }
    }

    #[inline(always)]
    unsafe fn disable_interrupts() {
        unsafe { asm!("msr daifset, #0b1111") }
    }

    unsafe fn interrupts_enabled() -> bool {
        DAIF.get() & 0b1111 != 0
    }

    #[inline(always)]
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

    #[inline(always)]
    unsafe fn invalidate_all() {
        unsafe {
            asm!("dsb ishst");
            asm!("tlbi vmalle1is");
            asm!("dsb ish");
            asm!("isb");
        }
    }

    #[inline(always)]
    unsafe fn current_page_table() -> PhysAddr {
        let addr: usize;
        unsafe {
            asm!("mrs {}, ttbr1_el1", out(reg) addr);
        }
        PhysAddr::new_canonical(addr)
    }

    #[inline(always)]
    unsafe fn set_current_page_table(addr: PhysAddr) {
        unsafe {
            asm!("dsb ishst");
            asm!("msr ttbr1_el1, {}", in(reg) addr.value());
            asm!("dsb ish");
            asm!("isb");
        }
    }

    #[inline(always)]
    unsafe fn set_stack_pointer_post_mapping(sp: VirtAddr) -> ! {
        unsafe {
            core::arch::asm!(
                "msr SPSel, #1",
                "mov sp, {}",
                "mov fp, xzr",
                "b {}",
                in(reg) sp.value(),
                sym crate::kernel_main_post_paging, options(noreturn)
            );
        }
    }

    #[inline(always)]
    fn instruction_pointer() -> usize {
        let pc: usize;
        unsafe {
            core::arch::asm!("mov {}, pc", out(reg) pc);
        }
        pc
    }

    #[inline(always)]
    fn stack_pointer() -> usize {
        let sp: usize;
        unsafe {
            core::arch::asm!("mov {}, sp", out(reg) sp);
        }
        sp
    }

    #[inline(always)]
    fn frame_pointer() -> usize {
        let fp: usize;
        unsafe {
            core::arch::asm!("mov {}, fp", out(reg) fp);
        }
        fp
    }

    fn exit_qemu(code: u32) -> ! {
        use qemu_exit::QEMUExit;
        qemu_exit::AArch64::new().exit(code)
    }

    fn hcf() -> ! {
        loop {
            unsafe {
                asm!("wfe");
                asm!("nop");
            }
        }
    }
}

#[repr(transparent)]
pub struct MmioDeref<T> {
    start_addr: usize,
    _marker: PhantomData<fn() -> T>,
}

impl<T> MmioDeref<T> {
    pub const unsafe fn new(start_addr: usize) -> Self {
        Self {
            start_addr,
            _marker: PhantomData,
        }
    }
}

impl<T> Deref for MmioDeref<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.start_addr as *const T) }
    }
}
