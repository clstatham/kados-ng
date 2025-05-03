use x86::{
    halt,
    msr::{IA32_PAT, wrmsr},
    tlb,
};
use x86_64::{
    instructions::interrupts,
    registers::control::{Cr3, Cr3Flags},
    structures::paging::PhysFrame,
};

use crate::mem::units::PhysAddr;

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

    unsafe fn pre_kernel_main_init() {
        gdt::init_boot();
    }

    unsafe fn init_mem() {
        let uncacheable = 0;
        let write_combining = 1;
        let write_through = 4;
        //let write_protected = 5;
        let write_back = 6;
        let uncached = 7;

        let pat0 = write_back;
        let pat1 = write_through;
        let pat2 = uncached;
        let pat3 = uncacheable;

        let pat4 = write_combining;
        let pat5 = pat1;
        let pat6 = pat2;
        let pat7 = pat3;

        unsafe {
            wrmsr(
                IA32_PAT,
                pat7 << 56
                    | pat6 << 48
                    | pat5 << 40
                    | pat4 << 32
                    | pat3 << 24
                    | pat2 << 16
                    | pat1 << 8
                    | pat0,
            )
        };
    }

    unsafe fn post_heap_init() {
        gdt::init();
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

    unsafe fn invalidate_page(addr: crate::mem::units::VirtAddr) {
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
        unsafe { PhysAddr::new_unchecked(cr3.start_address().as_u64() as usize) }
    }

    unsafe fn set_current_page_table(addr: PhysAddr) {
        let addr = unsafe { x86_64::PhysAddr::new_unsafe(addr.value() as u64) };
        unsafe { Cr3::write(PhysFrame::containing_address(addr), Cr3Flags::empty()) };
    }

    #[inline(always)]
    unsafe fn set_stack_pointer(sp: crate::mem::units::VirtAddr, next_fn: usize) -> ! {
        unsafe {
            core::arch::asm!("
                mov rsp, {sp}
                xor rbp, rbp
                jmp {next_fn}
            ", sp = in(reg) sp.value(), next_fn = in(reg) next_fn, options(noreturn))
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

    fn exit_qemu(code: u32) -> ! {
        todo!()
    }

    fn hcf() -> ! {
        loop {
            unsafe {
                halt();
            }
        }
    }
}
