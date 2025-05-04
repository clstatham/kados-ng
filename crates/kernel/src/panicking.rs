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
    println,
};

fn prevent_double_panic() {
    static PANICKING: AtomicBool = AtomicBool::new(false);

    if PANICKING.swap(true, Ordering::SeqCst) {
        // Already panicking, avoid infinite loop
        Arch::exit_qemu(1);
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    prevent_double_panic();

    println!("Panic: {}", info);

    if let Err(e) = unwind_kernel_stack() {
        println!("Error unwinding stack: {}", e);
    }

    Arch::hcf()
}

fn print_symbol(pc: usize, symtab: &[Entry64]) {
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
        rustc_demangle::demangle(name).as_str();
        println!("       {}", name);
    } else {
        println!("       <unknown>");
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

#[inline(always)]
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

    let mut fp = Arch::frame_pointer();
    let mut pc_ptr_opt = fp
        .checked_add(size_of::<usize>())
        .map(|p| p as *const usize);

    if fp == 0 {
        println!("<empty backtrace>");
        return Ok(());
    }

    let mapper = unsafe { Mapper::current() };

    println!("---BEGIN BACKTRACE---");
    for depth in 0..64 {
        if let Some(pc_ptr) = pc_ptr_opt {
            let fp_va = unsafe { VirtAddr::new_unchecked(fp) };
            let pc_va = unsafe { VirtAddr::new_unchecked(pc_ptr as usize) };
            let align_usize = align_of::<usize>();
            if fp_va.is_aligned(align_usize)
                && pc_va.is_aligned(align_usize)
                && mapper.translate(fp_va).is_ok()
                && mapper.translate(pc_va).is_ok()
            {
                let pc = unsafe { *pc_ptr };
                if pc == 0 {
                    println!("{:>2}: FP={}:  <empty return>", depth, fp_va);
                    break;
                } else {
                    println!("{:>2}: FP={} PC={}", depth, fp_va, pc_va);
                    print_symbol(pc, symtab);

                    fp = unsafe { *fp_va.as_raw_ptr::<usize>() };
                    pc_ptr_opt = fp
                        .checked_add(size_of::<usize>())
                        .map(|p| p as *const usize);
                }
            } else {
                println!("{:>2}: FP={}:  <guard page>", depth, fp_va);
                break;
            }
        } else {
            break;
        }
    }
    println!("---END BACKTRACE---");

    Ok(())
}
