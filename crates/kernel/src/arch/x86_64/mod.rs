use x86::{halt, tlb};
use x86_64::{
    instructions::interrupts,
    registers::control::{Cr3, Cr3Flags},
    structures::paging::PhysFrame,
};

use crate::mem::units::{PhysAddr, VirtAddr};

use super::ArchTrait;

pub mod gdt;
pub mod idt;
pub mod serial;
pub mod time;

pub struct X86_64;

impl ArchTrait for X86_64 {
    const PAGE_SHIFT: usize = 12;

    const PAGE_ENTRY_SHIFT: usize = 9;

    const PAGE_LEVELS: usize = 4;

    const PAGE_ENTRY_ADDR_WIDTH: usize = 40;

    const PAGE_FLAG_PAGE_DEFAULTS: usize = Self::PAGE_FLAG_PRESENT;

    const PAGE_FLAG_TABLE_DEFAULTS: usize = Self::PAGE_FLAG_PRESENT | Self::PAGE_FLAG_READWRITE;

    const PAGE_FLAG_PRESENT: usize = 1 << 0;

    const PAGE_FLAG_READONLY: usize = 0;

    const PAGE_FLAG_READWRITE: usize = 1 << 1;

    const PAGE_FLAG_USER: usize = 1 << 2;

    const PAGE_FLAG_EXECUTABLE: usize = 0;

    const PAGE_FLAG_NON_EXECUTABLE: usize = 1 << 63;

    const PAGE_FLAG_GLOBAL: usize = 1 << 8;

    const PAGE_FLAG_NON_GLOBAL: usize = 0;

    const PAGE_FLAG_HUGE: usize = 1 << 7;

    unsafe fn init_pre_kernel_main() {
        gdt::init_boot();
    }

    unsafe fn init_mem() {}

    unsafe fn init_post_heap() {
        gdt::init_post_heap();
    }

    unsafe fn init_interrupts() {
        idt::init();
    }

    unsafe fn enable_interrupts() {
        interrupts::enable();
    }

    unsafe fn disable_interrupts() {
        interrupts::disable();
    }

    unsafe fn interrupts_enabled() -> bool {
        interrupts::are_enabled()
    }

    unsafe fn invalidate_page(addr: VirtAddr) {
        unsafe {
            tlb::flush(addr.value());
        }
    }

    unsafe fn invalidate_all() {
        unsafe {
            tlb::flush_all();
        }
    }

    unsafe fn current_page_table() -> PhysAddr {
        let (cr3, _) = Cr3::read();
        PhysAddr::new_canonical(cr3.start_address().as_u64() as usize)
    }

    unsafe fn set_current_page_table(addr: PhysAddr) {
        let addr = unsafe { x86_64::PhysAddr::new_unsafe(addr.value() as u64) };
        unsafe { Cr3::write(PhysFrame::containing_address(addr), Cr3Flags::empty()) };
    }

    #[inline(always)]
    unsafe fn set_stack_pointer_post_mapping(sp: VirtAddr) -> ! {
        unsafe {
            core::arch::asm!("
                mov rsp, {sp}
                xor rbp, rbp
                jmp {next_fn}
            ", sp = in(reg) sp.value(), next_fn = sym crate::kernel_main_post_paging, options(noreturn))
        }
    }

    #[inline(always)]
    fn instruction_pointer() -> usize {
        let x: usize;
        unsafe {
            core::arch::asm!("mov {}, rip", out(reg) x);
        }
        x
    }

    #[inline(always)]
    fn stack_pointer() -> usize {
        let x: usize;
        unsafe {
            core::arch::asm!("mov {}, rsp", out(reg) x);
        }
        x
    }

    #[inline(always)]
    fn frame_pointer() -> usize {
        let x: usize;
        unsafe {
            core::arch::asm!("mov {}, rbp", out(reg) x);
        }
        x
    }

    fn exit_qemu(_code: u32) -> ! {
        Self::hcf() // todo
    }

    fn hcf() -> ! {
        loop {
            unsafe {
                halt();
            }
        }
    }
}
