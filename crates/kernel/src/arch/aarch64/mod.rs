use core::arch::asm;

use aarch64_cpu::registers::*;
use alloc::boxed::Box;
use serial::PERIPHERAL_BASE;

use crate::{
    cpu_local::CpuLocalBlock, dtb::IrqChipTrait, mem::{
        paging::{
            allocator::KernelFrameAllocator,
            table::{BlockSize, PageFlags, PageTable, TableKind},
        },
        units::{PhysAddr, VirtAddr},
    }, BOOT_INFO
};

use super::ArchTrait;

pub mod gpu;
pub mod gic;
pub mod boot;
pub mod serial;
pub mod syscall;
pub mod task;
pub mod time;
pub mod vectors;
pub mod mmio;

pub struct AArch64;

impl AArch64 {
    pub const PAGE_FLAG_NON_BLOCK: usize = 1 << 1;
    pub const PAGE_FLAG_ACCESS: usize = 1 << 10;
    pub const PAGE_FLAG_NORMAL: usize = 1 << 2;
    pub const PAGE_FLAG_INNER_SHAREABLE: usize = 0b11 << 8;
    pub const PAGE_FLAG_OUTER_SHAREABLE: usize = 0b10 << 8;

    pub const PAGE_FLAG_DEVICE: usize =
        Self::PAGE_FLAG_PRESENT      
            | Self::PAGE_FLAG_NON_BLOCK   
            | Self::PAGE_FLAG_ACCESS 
            | (0 << 2) // AttrIdx 0
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
        | Self::PAGE_FLAG_NON_BLOCK
        | Self::PAGE_FLAG_ACCESS
        | Self::PAGE_FLAG_NORMAL
        | Self::PAGE_FLAG_INNER_SHAREABLE;

    const PAGE_FLAG_TABLE_DEFAULTS: usize =
        Self::PAGE_FLAG_PRESENT | Self::PAGE_FLAG_NON_BLOCK;

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
        let page = frame.as_hhdm_virt();

        const PERIPHERAL_SIZE: usize = 0x200_0000;

        unsafe {
            let mut mapped = 0;
            while mapped < PERIPHERAL_SIZE {
                mapper.map_to(page.add_bytes(mapped), frame.add_bytes(mapped), BlockSize::Page4KiB, PageFlags::from_raw(Self::PAGE_FLAG_DEVICE)).unwrap().ignore();
                mapped += BlockSize::Page4KiB.size();
            }
        };

    }

    unsafe fn init_post_heap() {
        let boot_info = BOOT_INFO.get().unwrap();
        let fdt = boot_info.fdt.as_ref().unwrap();

        gpu::init(fdt);
    }

    unsafe fn init_interrupts() {}

    unsafe fn init_cpu_local_block() {
        unsafe {
            let frame = KernelFrameAllocator.allocate_one().unwrap();
            let virt = frame.as_hhdm_virt().as_raw_ptr_mut::<CpuLocalBlock>();
            let block = CpuLocalBlock::init();
            virt.write(block);
            TPIDR_EL1.set(virt as u64);
        }
    }

    unsafe fn init_syscalls() {}

    #[inline(always)]
    unsafe fn enable_interrupts() {
        DAIF.modify(DAIF::I::CLEAR);
        log::trace!("interrupts enabled");
    }

    #[inline(always)]
    unsafe fn disable_interrupts() {
        DAIF.modify(DAIF::D::SET);
        DAIF.modify(DAIF::A::SET);
        DAIF.modify(DAIF::I::SET);
        DAIF.modify(DAIF::F::SET);
        log::trace!("interrupts disabled");
    }

    unsafe fn interrupts_enabled() -> bool {
        !DAIF.is_set(DAIF::I) // IRQ flag NOT masked = IRQs enabled
    }

    #[inline(always)]
    unsafe fn invalidate_page(addr: VirtAddr) {
        unsafe {
            asm!("
            dc cvau, {0}
            dsb ish
            tlbi vae1is, {0}
            dsb sy
            isb
        ", in(reg) addr.value());
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
            match kind {
                TableKind::Kernel => {
                    asm!(
                        "dsb sy",
                        "msr ttbr1_el1, {0}",
                        "isb",
                        "dsb ishst",
                        "tlbi vmalle1is",
                        "dsb ish",
                        "isb",
                        in(reg) addr.value(),
                        options(nostack),
                    );
                }
                TableKind::User => {
                    asm!(
                        "dsb sy",
                        "msr ttbr0_el1, {0}",
                        "isb",
                        "dsb ishst",
                        "tlbi vmalle1is",
                        "dsb ish",
                        "isb",
                        in(reg) addr.value(),
                        options(nostack),
                    );
                }
            }
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

    fn new_irq_chip(compatible: &str) -> Option<Box<dyn IrqChipTrait>> {
        if compatible.contains("arm,gic-400") {
            Some(Box::new(gic::Gic::default()))
        } else {
            log::warn!("No interrupt chip driver for {compatible}");
            None
        }
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
