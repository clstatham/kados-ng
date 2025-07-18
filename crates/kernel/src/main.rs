#![no_std]
#![no_main]
#![allow(
    clippy::missing_safety_doc,
    clippy::new_without_default,
    clippy::uninlined_format_args,
    clippy::identity_op,
    clippy::unnecessary_cast,
    clippy::eq_op,
    clippy::missing_errors_doc,
    clippy::cast_possible_truncation, // todo: fix instances and remove this
    clippy::cast_possible_wrap, // todo: fix instances and remove this
    clippy::cast_sign_loss, // todo: fix instances and remove this
)]
#![feature(if_let_guard, iter_next_chunk, array_chunks)]

use arch::{Arch, Architecture};
use fdt::Fdt;
use mem::paging::{
    MemMapEntries,
    allocator::{init_kernel_frame_allocator, kernel_frame_allocator},
};
use spin::Once;

extern crate alloc;

pub mod arch;
pub mod cpu_local;
pub mod fdt;
pub mod logging;
pub mod syscall;
pub mod task;
pub mod time;
#[macro_use]
pub mod util;
#[macro_use]
pub mod framebuffer;
pub mod irq;
pub mod mem;
pub mod panicking;
pub mod sync;

/// Boot information structure.
#[repr(C)]
pub struct BootInfo {
    /// The flattened device tree blob, if available.
    pub fdt: Option<Fdt<'static>>,

    /// The memory map entries determined by the bootloader.
    pub mem_map: MemMapEntries<32>,
}

/// The boot information structure, initialized by the bootloader.
pub static BOOT_INFO: Once<BootInfo> = Once::new();

/// The offset between physical and virtual addresses when mapped linearly.
pub const HHDM_PHYSICAL_OFFSET: usize = 0xffff_8000_0000_0000;

/// The base address of the kernel in virtual memory.
///
/// This must match the value in the linker script.
pub const KERNEL_OFFSET: usize = 0xffff_ffff_8000_0000;

macro_rules! elf_offsets {
    ($($name:ident),* $(,)?) => {
        $(
            #[inline]
            #[doc = concat!("Returns the address of the kernel ELF symbol `", stringify!($name), "`.")]
            #[must_use]
            pub fn $name() -> usize {
                unsafe extern "C" {
                    unsafe static $name: u8;
                }
                &raw const $name as usize
            }
        )*
    };
}

elf_offsets!(
    __boot_start,
    __text_start,
    __exception_vectors,
    __text_end,
    __rodata_start,
    __rodata_end,
    __data_start,
    __data_end,
    __bss_start,
    __bss_end,
    __kernel_phys_start,
    __kernel_phys_end,
    __stack_bottom,
    __stack_top,
);

/// The entry point for the kernel.
///
/// This function is called by the bootloader after it has set up the CPU and memory.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn kernel_main() -> ! {
    unsafe {
        Arch::disable_interrupts();

        Arch::init_pre_kernel_main();
    }

    let boot_info = BOOT_INFO.get().unwrap();

    for _ in 0..3 {
        println!();
    }

    logging::init();

    log::info!("kernel starting...");

    init_kernel_frame_allocator(boot_info);

    log::info!("initializing memory...");
    unsafe {
        mem::paging::map_memory(boot_info);
    }

    log::info!("initializing interrupts...");

    unsafe {
        Arch::init_interrupts();
    }

    log::info!("initializing heap...");
    unsafe {
        mem::heap::init_heap();
    }

    log::info!("initializing frame allocator (post-heap)...");
    kernel_frame_allocator().convert_post_heap().unwrap();

    log::info!("initializing device tree...");
    let fdt = boot_info.fdt.as_ref().unwrap();
    fdt::init(fdt);

    log::info!("initializing irq chip...");
    irq::init(fdt);

    log::info!("initializing per-cpu structure...");
    unsafe {
        Arch::init_cpu_local_block();
    }

    log::info!("initializing timer...");
    arch::time::init(fdt);

    log::info!("running init hooks (post-heap)...");
    unsafe {
        Arch::init_drivers();
    }

    log::info!("initializing framebuffer...");
    crate::framebuffer::init();

    log::info!("initializing task contexts...");
    task::context::init();

    log::info!("spawning first task...");

    task::spawn(false, test).unwrap();

    #[rustfmt::skip]
    println!(
        r"
welcome to...
 ___  __    ________  ________  ________  ________      
|\  \|\  \ |\   __  \|\   ___ \|\   __  \|\   ____\     
\ \  \/  /|\ \  \|\  \ \  \_|\ \ \  \|\  \ \  \___|_    
 \ \   ___  \ \   __  \ \  \ \\ \ \  \\\  \ \_____  \   
  \ \  \\ \  \ \  \ \  \ \  \_\\ \ \  \\\  \|____|\  \  
   \ \__\\ \__\ \__\ \__\ \_______\ \_______\____\_\  \ 
    \|__| \|__|\|__|\|__|\|_______|\|_______|\_________\
                                            \|_________|

"
    );

    unsafe { Arch::enable_interrupts() }

    Arch::hcf()
}

extern "C" fn test() {
    log::warn!("Hello from PID 1!");
    task::context::exit_current();
}

/// Prints a formatted string to the serial console and framebuffer.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        let _ = $crate::arch::serial::write_fmt(format_args!($($arg)*));
        let _ = $crate::framebuffer::write_fmt(format_args!($($arg)*));
    });
}

/// Prints a formatted string to the serial console.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        let _ = $crate::arch::serial::write_fmt(format_args!($($arg)*));
    };
}

/// Prints a formatted string to the serial console and framebuffer, followed by a newline.
#[macro_export]
macro_rules! println {
    () => ({
        let _ = $crate::arch::serial::write_fmt(format_args!("\n"));
        let _ = $crate::framebuffer::write_fmt(format_args!("\n"));
    });
    ($($arg:tt)*) => ({
        let _ = $crate::arch::serial::write_fmt(format_args!($($arg)*));
        let _ = $crate::arch::serial::write_fmt(format_args!("\n"));
        let _ = $crate::framebuffer::write_fmt(format_args!($($arg)*));
        let _ = $crate::framebuffer::write_fmt(format_args!("\n"));
    });
}

/// Prints a formatted string to the serial console, followed by a newline.
#[macro_export]
macro_rules! serial_println {
    () => ({
        let _ = $crate::arch::serial::write_fmt(format_args!("\n"));
    });
    ($($arg:tt)*) => ({
        let _ = $crate::arch::serial::write_fmt(format_args!($($arg)*));
        let _ = $crate::arch::serial::write_fmt(format_args!("\n"));
    });
}
