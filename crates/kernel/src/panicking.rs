use core::{
    fmt::Write,
    sync::atomic::{AtomicBool, Ordering},
};

use arrayvec::ArrayString;
use thiserror::Error;

use crate::{
    arch::{serial::lock_uart, Arch, ArchTrait},
    mem::{
        paging::table::{PageTable, TableKind},
        units::VirtAddr,
    },
    println,
};

fn prevent_double_panic() {
    static PANICKING: AtomicBool = AtomicBool::new(false);

    if PANICKING.swap(true, Ordering::SeqCst) {
        // Already panicking, avoid infinite loop
        Arch::hcf()
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
    let mut fp = Arch::frame_pointer();
    let mut pc_ptr_opt = fp
        .checked_add(size_of::<usize>())
        .map(|p| p as *const usize);

    if fp == 0 {
        println!("<empty backtrace>");
        return Ok(());
    }

    let mapper = PageTable::current(TableKind::Kernel);

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
                    let name = symbol_name(pc);

                    if let Some(name) = name {
                        println!("       {}", rustc_demangle::demangle(&name));
                    } else {
                        println!("       <unknown>");
                    }

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

pub fn symbol_name(addr: usize) -> Option<ArrayString<2048>> {
    let mut uart = lock_uart();
    uart.write_fmt(format_args!("[sym?]{}\n", addr)).ok()?;
    let mut out = ArrayString::new();
    loop {
        let b = uart.getchar();
        if b == b'\n' {
            break;
        }
        if let Ok(s) = str::from_utf8(&[b]) {
            if out.try_push_str(s).is_err() {
                break;
            }
        } else {
            break;
        }
    }

    Some(out)
}
