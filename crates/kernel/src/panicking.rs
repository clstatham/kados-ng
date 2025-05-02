use core::sync::atomic::{AtomicBool, Ordering};

use thiserror::Error;
use xmas_elf::{
    sections::{SectionData, ShType},
    symbol_table::{Entry, Entry64},
};

use crate::{
    KERNEL_ELF,
    arch::{Arch, ArchTrait},
    mem::{
        paging::{allocator::KernelFrameAllocator, mapper::Mapper},
        units::VirtAddr,
    },
};

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

fn prevent_double_panic() {
    static PANICKING: AtomicBool = AtomicBool::new(false);

    if PANICKING.swap(true, Ordering::SeqCst) {
        // Already panicking, avoid infinite loop
        Arch::exit_qemu(1);
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    prevent_double_panic();

    println!("Panic: {}", info);

    if let Err(e) = unwind_kernel_stack() {
        println!("Error unwinding stack: {}", e);
    }

    Arch::exit_qemu(1);
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    prevent_double_panic();

    println!("[failed]");
    println!("Panic: {}", info);
    Arch::exit_qemu(1);
}

fn print_symbol(pc: usize, symtab: &[Entry64], depth: usize, demangle: bool) {
    let kernel_elf = KERNEL_ELF.get().unwrap();
    let mut name = None;
    for entry in symtab.iter() {
        let value = entry.value() as usize;
        let size = entry.size() as usize;

        if pc >= value && pc < (value + size) {
            name = entry.get_name(kernel_elf).ok();
            break;
        }
    }

    if let Some(name) = name {
        let name = if demangle {
            rustc_demangle::demangle(name).as_str()
        } else {
            name
        };
        println!("{:>2}: 0x{:016x} - {}", depth, pc, name);
    } else {
        println!("{:>2}: 0x{:016x} - <unknown>", depth, pc);
    }
}

#[derive(Debug, Error)]
pub enum UnwindStackError {
    #[error("Kernel ELF file not initialized")]
    KernelElfNotInitialized,
    #[error("No kernel symbol table available")]
    NoSymbolTable,
    #[error("Failed to get kernel section data")]
    FailedToGetSectionData,
}

pub fn unwind_kernel_stack() -> Result<(), UnwindStackError> {
    let kernel_elf = KERNEL_ELF
        .get()
        .ok_or(UnwindStackError::KernelElfNotInitialized)?;

    let mut symtab = None;

    for section in kernel_elf.section_iter() {
        if section.get_type() == Ok(ShType::SymTab) {
            let section_data = section
                .get_data(kernel_elf)
                .map_err(|_| UnwindStackError::FailedToGetSectionData)?;

            if let SectionData::SymbolTable64(s) = section_data {
                symtab = Some(s);
                break;
            }
        }
    }

    let symtab = symtab.ok_or(UnwindStackError::NoSymbolTable)?;

    let mut fp: usize;
    unsafe {
        core::arch::asm!("mov {}, fp", out(reg) fp);
    }

    if fp == 0 {
        println!("<empty backtrace>");
        return Ok(());
    }

    let mapper = unsafe { Mapper::current(KernelFrameAllocator) };

    println!("---BEGIN BACKTRACE---");
    for depth in 0..16 {
        if let Some(pc_fp) = fp.checked_add(size_of::<usize>()) {
            let pc_fp = unsafe { VirtAddr::new_unchecked(pc_fp) };
            if mapper.translate(pc_fp).is_err() {
                println!("{:>2}: <guard page>", depth);
                break;
            }

            let pc = unsafe { pc_fp.read::<usize>().unwrap_or(0) };
            if pc == 0 || fp == 0 {
                break;
            }

            unsafe {
                fp = *(fp as *const usize);
            }

            print_symbol(pc, symtab, depth, true);
        } else {
            break;
        }
    }
    println!("---END BACKTRACE---");

    Ok(())
}
