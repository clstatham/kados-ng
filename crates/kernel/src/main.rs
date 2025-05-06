#![no_std]
#![no_main]
#![allow(
    clippy::missing_safety_doc,
    clippy::new_without_default,
    clippy::uninlined_format_args
)]

use arch::{Arch, ArchTrait};
use cpu_local::CpuLocalBlock;
use framebuffer::FramebufferInfo;
use limine::{
    memory_map::EntryType,
    request::{
        DateAtBootRequest, EntryPointRequest, ExecutableFileRequest, FramebufferRequest,
        HhdmRequest, MemoryMapRequest, StackSizeRequest,
    },
};
use mem::{
    paging::{
        MEM_MAP_ENTRIES, MemMapEntries, MemMapEntry,
        allocator::{init_kernel_frame_allocator, kernel_frame_allocator},
    },
    units::{FrameCount, PhysAddr},
};
use spin::Once;
use task::{
    context::{CONTEXTS, Context, ContextRef},
    switch::SwitchResult,
};
use xmas_elf::ElfFile;

extern crate alloc;

pub mod arch;
pub mod cpu_local;
pub mod logging;
pub mod serial;
pub mod syscall;
pub mod task;
#[macro_use]
pub mod framebuffer;
pub mod mem;
pub mod panicking;
pub mod sync;

static HHDM: HhdmRequest = HhdmRequest::new();
static _ENTRY_POINT: EntryPointRequest = EntryPointRequest::new().with_entry_point(kernel_main);
static _STACK: StackSizeRequest = StackSizeRequest::new().with_size(KERNEL_STACK_SIZE as u64);
static BOOT_TIME: DateAtBootRequest = DateAtBootRequest::new();
static MEM_MAP: MemoryMapRequest = MemoryMapRequest::new();
static KERNEL_FILE: ExecutableFileRequest = ExecutableFileRequest::new();
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

static KERNEL_ELF_PHYSADDR: Once<PhysAddr> = Once::new();
static KERNEL_ELF_SIZE: Once<usize> = Once::new();
static KERNEL_ELF: Once<ElfFile<'static>> = Once::new();

static FRAMEBUFFER_INFO: Once<FramebufferInfo> = Once::new();

pub const KERNEL_OFFSET: usize = 0xffffffff80000000; // must match linker.ld
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
    __text_start,
    __exception_vectors,
    __text_end,
    __rodata_start,
    __rodata_end,
    __data_start,
    __data_end,
    __bss_start,
    __bss_end,
);

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    unsafe { Arch::disable_interrupts() };

    let hhdm = HHDM.get_response().unwrap();
    mem::HHDM_PHYSICAL_OFFSET.call_once(|| hhdm.offset() as usize);

    unsafe {
        Arch::init_pre_kernel_main();
    }

    let boot_time = BOOT_TIME.get_response().unwrap();

    arch::serial::init();

    unsafe {
        arch::time::init(boot_time.timestamp());
    }

    logging::init();

    log::info!("HHDM offset: {:#016x}", hhdm.offset());

    let kernel_file = KERNEL_FILE.get_response().unwrap();
    let kernel_file = kernel_file.file();
    let kernel_elf_physaddr = kernel_file.addr() as usize - hhdm.offset() as usize;
    KERNEL_ELF_PHYSADDR.call_once(|| PhysAddr::new_canonical(kernel_elf_physaddr));
    KERNEL_ELF_SIZE.call_once(|| kernel_file.size() as usize);

    let fb_tag = FRAMEBUFFER_REQUEST.get_response().unwrap();
    let fb0 = fb_tag.framebuffers().next().unwrap();
    FRAMEBUFFER_INFO.call_once(|| FramebufferInfo {
        base: fb0.addr() as usize,
        width: fb0.width() as usize,
        height: fb0.height() as usize,
        bpp: fb0.bpp() as usize,
    });

    log::info!("Kernel starting...");

    let mem_map = MEM_MAP.get_response().unwrap();
    let mut total_free = 0;
    let mut mem_map_entries = MemMapEntries::new();

    for entry in mem_map.entries().iter() {
        let description = match entry.entry_type {
            EntryType::USABLE => "USABLE",
            EntryType::RESERVED => "RESERVED",
            EntryType::ACPI_NVS => "ACPI_NVS",
            EntryType::ACPI_RECLAIMABLE => "ACPI_RECLAIMABLE",
            EntryType::BAD_MEMORY => "BAD_MEMORY",
            EntryType::BOOTLOADER_RECLAIMABLE => "BOOTLOADER_RECLAIMABLE",
            EntryType::EXECUTABLE_AND_MODULES => "EXECUTABLE_AND_MODULES",
            EntryType::FRAMEBUFFER => "FRAMEBUFFER",
            _ => "UNKNOWN",
        };
        log::info!(
            "{:#016x} .. {:#016x}  {}",
            entry.base,
            entry.base + entry.length,
            description
        );

        match entry.entry_type {
            EntryType::USABLE => {
                total_free += entry.length;
                mem_map_entries.push_usable(MemMapEntry {
                    base: PhysAddr::new_canonical(entry.base as usize),
                    size: FrameCount::from_bytes(entry.length as usize),
                    kind: entry.entry_type,
                });
            }
            EntryType::BOOTLOADER_RECLAIMABLE
            | EntryType::ACPI_RECLAIMABLE
            | EntryType::FRAMEBUFFER => {
                mem_map_entries.push_identity_map(MemMapEntry {
                    base: PhysAddr::new_canonical(entry.base as usize),
                    size: FrameCount::from_bytes(entry.length as usize),
                    kind: entry.entry_type,
                });
            }
            EntryType::EXECUTABLE_AND_MODULES => mem_map_entries.set_kernel_entry(MemMapEntry {
                base: PhysAddr::new_canonical(entry.base as usize),
                size: FrameCount::from_bytes(entry.length as usize),
                kind: entry.entry_type,
            }),
            _ => {}
        }
    }
    log::info!("{total_free} bytes free");
    MEM_MAP_ENTRIES.call_once(|| mem_map_entries);

    log::info!("Adding memory map to kernel frame allocator");
    init_kernel_frame_allocator(MEM_MAP_ENTRIES.get().unwrap().usable_entries());

    log::info!("Initializing memory");
    mem::paging::map_memory()
}

pub extern "C" fn kernel_main_post_paging() -> ! {
    unsafe {
        Arch::invalidate_all();
    }

    log::info!("Initializing kernel ELF file info");
    let kernel_file_addr = KERNEL_ELF_PHYSADDR.get().unwrap().as_hhdm_virt();
    let kernel_file_size = *KERNEL_ELF_SIZE.get().unwrap();
    let kernel_file_data =
        unsafe { core::slice::from_raw_parts(kernel_file_addr.as_raw_ptr(), kernel_file_size) };
    KERNEL_ELF.call_once(|| ElfFile::new(kernel_file_data).expect("Error parsing kernel ELF file"));

    log::info!("Initializing interrupts");

    unsafe {
        Arch::init_interrupts();
    }

    log::info!("Initializing per-cpu structure");
    unsafe {
        Arch::init_cpu_local_block();
    }

    mem::heap::init_heap();

    log::info!("Initializing memory (post-heap)");

    unsafe {
        Arch::init_post_heap();
    }

    log::info!("Initializing framebuffer");
    framebuffer::init(*FRAMEBUFFER_INFO.get().unwrap());

    log::info!("Initializing frame allocator (post-heap)");
    kernel_frame_allocator().lock().convert_post_heap().unwrap();

    log::info!("Initializing first context");
    task::context::init();

    task::spawn(false, test).unwrap();

    log::info!("Kernel boot finished after {:?}", arch::time::uptime());
    log::info!("Welcome to KaDOS!");

    loop {
        unsafe {
            Arch::enable_interrupts();
            Arch::halt();
        }
    }
}

extern "C" fn test() {
    log::warn!("This should only run once!");

    let cx = task::context::current().unwrap();
    CONTEXTS.write().remove(&ContextRef(cx));
    task::switch::switch();
    unreachable!()
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        let _ = $crate::serial::_print(format_args!($($arg)*));
        let _ = $crate::framebuffer::_fb_print(format_args!($($arg)*));
    });
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => ({
        let _ = $crate::serial::_print(format_args!($($arg)*));
        let _ = $crate::framebuffer::write_fmt(format_args!($($arg)*));
        $crate::serial::write_fmt(format_args!("\n"));
        $crate::framebuffer::write_fmt(format_args!("\n"));
    });
}
