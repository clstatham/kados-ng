use core::arch::asm;

use fdt::Fdt;

use crate::{
    BOOT_INFO, BootInfo,
    arch::{Arch, Architecture},
    mem::{
        paging::{MemMapEntries, MemMapEntry},
        units::{FrameCount, PhysAddr},
    },
    println,
};

unsafe extern "C" {
    unsafe static __boot_start: u8;
    unsafe static __boot_end: u8;
    unsafe static __kernel_phys_start: u8;
    unsafe static __kernel_phys_end: u8;
    unsafe static __kernel_virt_start: u8;
    unsafe static __bss_start: u8;
    unsafe static __bss_end: u8;
    unsafe static __kernel_virt_end: u8;

}

unsafe fn memzero(start: usize, end: usize) {
    unsafe {
        asm!(
            "
        mov x1, {start}
        mov x2, {end}
        mov x3, xzr
    1:
        cmp x1, x2
        b.hs 2f
        str x3, [x1], #8
        b 1b
    2:
        dsb sy
        isb
        ",
        start = in(reg) start,
        end = in(reg) end,
        out("x1") _,
        out("x2") _,
        out("x3") _,
        )
    }
}

/// The higher-half boot function.
///
/// This function is called by the bootloader to initialize the kernel in higher-half memory.
/// It sets up the BSS section, parses the flattened device tree (FDT),
/// and calls the `kernel_main` function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boot_higher_half(dtb_ptr: *const u8) -> ! {
    unsafe {
        super::serial::init();
        let bss_start = &__bss_start as *const u8 as usize;
        let bss_end = &__bss_end as *const u8 as usize;

        println!();

        println!("zeroing BSS 0x{:016x} .. 0x{:016x}", bss_start, bss_end);
        memzero(bss_start, bss_end);

        println!("parsing FDT");
        let fdt = Fdt::from_ptr(dtb_ptr).unwrap();
        let mut mem_map = MemMapEntries::new();

        let kernel_phys_start = &__kernel_phys_start as *const _ as usize;
        let kernel_phys_end = &__kernel_phys_end as *const _ as usize;
        let boot_phys_start = &__boot_start as *const _ as usize;
        let boot_phys_end = &__boot_end as *const _ as usize;

        println!("enumerating memory regions");
        for region in fdt.memory().regions() {
            let mut start = (region.starting_address as usize).max(boot_phys_start);
            let end = start + region.size.unwrap_or(0);
            if start >= end {
                continue;
            }
            let mut page = start;
            while page < end {
                if (kernel_phys_start..kernel_phys_end).contains(&page) {
                    // we've run into kernel code; end our current chunk and skip past it
                    if page > start {
                        mem_map.push_usable(MemMapEntry {
                            base: PhysAddr::new_canonical(start),
                            size: FrameCount::from_bytes(page - start),
                        });
                    }

                    start = kernel_phys_end;
                    page = kernel_phys_end;
                    continue;
                }
                if (boot_phys_start..boot_phys_end).contains(&page) {
                    // we've run into boot code; end our current chunk and skip past it
                    if page > start {
                        mem_map.push_usable(MemMapEntry {
                            base: PhysAddr::new_canonical(start),
                            size: FrameCount::from_bytes(page - start),
                        });
                    }

                    start = boot_phys_end;
                    page = boot_phys_end;
                    continue;
                }
                page += Arch::PAGE_SIZE;
            }
            if start < end {
                // we've run out of space; add the remaining chunk
                mem_map.push_usable(MemMapEntry {
                    base: PhysAddr::new_canonical(start),
                    size: FrameCount::from_bytes(end - start),
                });
            }
        }

        let boot_info = BootInfo {
            fdt: Some(fdt),
            mem_map,
        };

        BOOT_INFO.call_once(|| boot_info);

        println!("calling kernel_main");
        crate::kernel_main()
    }
}
