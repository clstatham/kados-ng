#![no_std]
#![no_main]
#![allow(
    clippy::missing_safety_doc,
    clippy::new_without_default,
    clippy::uninlined_format_args,
    clippy::identity_op,
    clippy::unnecessary_cast,
    clippy::eq_op
)]
#![feature(if_let_guard, iter_next_chunk, array_chunks)]

use arch::{Arch, ArchTrait};
use fdt::Fdt;
use mem::paging::{
    MemMapEntries,
    allocator::{init_kernel_frame_allocator, kernel_frame_allocator},
};
use spin::Once;
use xmas_elf::ElfFile;

extern crate alloc;

pub mod arch;
pub mod cpu_local;
pub mod dtb;
pub mod logging;
pub mod syscall;
pub mod task;
pub mod time;
#[macro_use]
pub mod util;
#[macro_use]
pub mod framebuffer;
pub mod mem;
pub mod panicking;
pub mod sync;

#[repr(C)]
pub struct BootInfo {
    pub fdt: Option<Fdt<'static>>,
    pub mem_map: MemMapEntries<32>,
}

pub static BOOT_INFO: Once<BootInfo> = Once::new();

static KERNEL_ELF: Once<ElfFile<'static>> = Once::new();

pub const HHDM_PHYSICAL_OFFSET: usize = 0xffff_8000_0000_0000;
pub const KERNEL_OFFSET: usize = 0xffff_ffff_8000_0000; // must match linker.ld
pub const KERNEL_STACK_SIZE: usize = 1024 * 1024 * 2;

macro_rules! elf_offsets {
    ($($name:ident),* $(,)?) => {
        $(
            pub fn $name() -> usize {
                unsafe extern "C" {
                    unsafe static $name: u8;
                }
                unsafe { &$name as *const u8 as usize }
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
    dtb::init(fdt);

    log::info!("initializing per-cpu structure...");
    unsafe {
        Arch::init_cpu_local_block();
    }

    log::info!("initializing timer...");
    arch::time::init(fdt);

    unsafe { Arch::enable_interrupts() };

    log::info!("running init hooks (post-heap)...");
    unsafe {
        Arch::init_drivers();
    }

    log::info!("initializing framebuffer...");
    crate::framebuffer::init();

    unsafe {
        Arch::disable_interrupts();
    }

    log::info!("initializing task contexts...");
    task::context::init();

    log::info!("spawning first task...");

    task::spawn(false, test).unwrap();

    #[rustfmt::skip]
    println!(
        r#"
welcome to...
 ___  __    ________  ________  ________  ________      
|\  \|\  \ |\   __  \|\   ___ \|\   __  \|\   ____\     
\ \  \/  /|\ \  \|\  \ \  \_|\ \ \  \|\  \ \  \___|_    
 \ \   ___  \ \   __  \ \  \ \\ \ \  \\\  \ \_____  \   
  \ \  \\ \  \ \  \ \  \ \  \_\\ \ \  \\\  \|____|\  \  
   \ \__\\ \__\ \__\ \__\ \_______\ \_______\____\_\  \ 
    \|__| \|__|\|__|\|__|\|_______|\|_______|\_________\
                                            \|_________|

"#
    );

    unsafe { Arch::enable_interrupts() }

    Arch::hcf();
}

extern "C" fn test() {
    log::warn!("Hello from PID 1!");
    task::context::exit_current();
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        let _ = $crate::arch::serial::write_fmt(format_args!($($arg)*));
        let _ = $crate::framebuffer::write_fmt(format_args!($($arg)*));
    });
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        let _ = $crate::arch::serial::write_fmt(format_args!($($arg)*));
    };
}

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
