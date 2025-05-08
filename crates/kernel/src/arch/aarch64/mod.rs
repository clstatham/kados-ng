use core::arch::asm;

use aarch64_cpu::registers::*;
use serial::PERIPHERAL_BASE;

use crate::{
    cpu_local::CpuLocalBlock,
    mem::{
        paging::{
            allocator::KernelFrameAllocator,
            table::{PageFlags, PageTable, TableKind},
        },
        units::{PhysAddr, VirtAddr},
    },
};

use super::ArchTrait;

pub mod boot;
pub mod gic;
pub mod random;
pub mod serial;
pub mod syscall;
pub mod task;
pub mod time;
pub mod vectors;

pub struct AArch64;

impl AArch64 {
    pub const PAGE_FLAG_TYPE: usize = 1 << 1;
    pub const PAGE_FLAG_ACCESS: usize = 1 << 10;
    pub const PAGE_FLAG_NORMAL: usize = 1 << 2;
    pub const PAGE_FLAG_INNER_SHAREABLE: usize = 0b11 << 8;
    pub const PAGE_FLAG_OUTER_SHAREABLE: usize = 0b10 << 8;

    pub const PAGE_FLAG_DEVICE: usize =
        Self::PAGE_FLAG_PRESENT      
            | Self::PAGE_FLAG_TYPE   
            | Self::PAGE_FLAG_ACCESS 
            | (0 << 2) // AttrIdx (0, nGnRE)
            | (0 << 6) // AP (RW, priv)
            | Self::PAGE_FLAG_OUTER_SHAREABLE
            | Self::PAGE_FLAG_NON_EXECUTABLE;
}

impl ArchTrait for AArch64 {
    const PAGE_SHIFT: usize = 12;

    const PAGE_ENTRY_SHIFT: usize = 9;

    const PAGE_LEVELS: usize = 4;

    const PAGE_ENTRY_ADDR_WIDTH: usize = 40;

    const PAGE_FLAG_PAGE_DEFAULTS: usize = Self::PAGE_FLAG_PRESENT
        | Self::PAGE_FLAG_TYPE
        | Self::PAGE_FLAG_ACCESS
        | Self::PAGE_FLAG_NORMAL
        | Self::PAGE_FLAG_INNER_SHAREABLE;

    const PAGE_FLAG_TABLE_DEFAULTS: usize =
        Self::PAGE_FLAG_PRESENT | Self::PAGE_FLAG_READWRITE | Self::PAGE_FLAG_TYPE;

    const PAGE_FLAG_PRESENT: usize = 1 << 0;

    const PAGE_FLAG_READONLY: usize = 1 << 7;

    const PAGE_FLAG_READWRITE: usize = 0;

    const PAGE_FLAG_USER: usize = 1 << 6;

    const PAGE_FLAG_EXECUTABLE: usize = 0;

    const PAGE_FLAG_NON_EXECUTABLE: usize = 0b11 << 53;

    const PAGE_FLAG_GLOBAL: usize = 0;

    const PAGE_FLAG_NON_GLOBAL: usize = 1 << 11;

    const PAGE_FLAG_HUGE: usize = 0;

    #[inline(always)]
    unsafe fn init_pre_kernel_main() {}

    unsafe fn init_mem(mapper: &mut PageTable) {
        let frame = PhysAddr::new_canonical(PERIPHERAL_BASE);
        let page = VirtAddr::new_canonical(PERIPHERAL_BASE);

        const PERIPHERAL_SIZE: usize = 2 * 1024 * 1024;

        unsafe {
            mapper
                .kernel_map_range(
                    page,
                    frame,
                    PERIPHERAL_SIZE,
                    PageFlags::from_raw(Self::PAGE_FLAG_DEVICE),
                )
                .unwrap()
                .ignore();
        };

        unsafe {
            gic::init();
        }
    }

    unsafe fn init_post_heap() {}

    #[inline(never)]
    unsafe fn init_interrupts() {
        unsafe {
            vectors::init();
        }
    }

    unsafe fn init_cpu_local_block() {
        unsafe {
            log::debug!("allocate_one");
            let frame = KernelFrameAllocator.allocate_one().unwrap();
            let virt = frame.as_hhdm_virt().as_raw_ptr_mut::<CpuLocalBlock>();
            log::debug!("init");
            let block = CpuLocalBlock::init();
            log::debug!("write");
            virt.write(block);
            log::debug!("set");
            TPIDR_EL1.set(virt as u64);
        }
    }

    unsafe fn init_syscalls() {}

    #[inline(always)]
    unsafe fn enable_interrupts() {
        unsafe { asm!("msr daifclr, #0b1111") }
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
    unsafe fn current_page_table(kind: TableKind) -> PhysAddr {
        let addr: usize;
        unsafe {
            match kind {
                TableKind::Kernel => asm!("mrs {}, ttbr1_el1", out(reg) addr),
                TableKind::User => asm!("mrs {}, ttbr0_el1", out(reg) addr),
            }
        }
        PhysAddr::new_canonical(addr)
    }

    #[inline(always)]
    unsafe fn set_current_page_table(addr: PhysAddr, kind: TableKind) {
        unsafe {
            asm!("dsb ishst");
            match kind {
                TableKind::Kernel => asm!("msr ttbr1_el1, {}", in(reg) addr.value()),
                TableKind::User => asm!("msr ttbr0_el1, {}", in(reg) addr.value()),
            }
            Self::invalidate_all();
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

    fn current_cpu_local_block() -> VirtAddr {
        VirtAddr::new_canonical(TPIDR_EL1.get() as usize)
    }

    fn emergency_reset() -> ! {
        unsafe {
            asm!("hvc   #0",
                 in("x0")  0x8400_0009_usize,
                 options(noreturn),
            )
        }
    }

    fn exit_qemu(code: u32) -> ! {
        use qemu_exit::QEMUExit;
        qemu_exit::AArch64::new().exit(code)
    }

    #[inline(always)]
    fn halt() {
        unsafe { asm!("wfe") }
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
