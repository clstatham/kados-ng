use core::arch::{asm, global_asm};

use fdt::Fdt;

use crate::{
    BOOT_INFO, BootInfo,
    arch::{Arch, ArchTrait},
    mem::{
        paging::{MemMapEntries, MemMapEntry},
        units::{FrameCount, PhysAddr},
    },
    serial::PERIPHERAL_BASE,
};

global_asm!(include_str!("boot.S"));

unsafe extern "C" {
    unsafe static __boot_start: u8;
    unsafe static __boot_stack_bottom: u8;
    unsafe static __boot_stack_top: u8;
    unsafe static __boot_table: u8;
    unsafe static __boot_table_end: u8;
    unsafe static __boot_end: u8;
    unsafe static __kernel_phys_start: u8;
    unsafe static __kernel_phys_end: u8;
    unsafe static __kernel_virt_start: u8;
    unsafe static __boot_table_higher: u8;
    unsafe static __kernel_virt_end: u8;
}

#[repr(C, align(4096))]
pub struct Table([usize; 512]);

#[unsafe(no_mangle)]
#[unsafe(link_section = ".boot")]
pub unsafe extern "C" fn mmu_init_el2(dtb_ptr: *const u8, _zero: usize) -> ! {
    unsafe {
        if core_affinity() != 0 {
            loop {
                asm!("wfe");
            }
        }

        let mut off = &__boot_table as *const _ as usize;

        let l0 = alloc_table(&mut off);

        let flags = Arch::PAGE_FLAG_ACCESS
            | Arch::PAGE_FLAG_INNER_SHAREABLE
            | Arch::PAGE_FLAG_NON_BLOCK
            | Arch::PAGE_FLAG_NORMAL
            | Arch::PAGE_FLAG_PRESENT;

        map_range(
            &mut off,
            l0,
            0,
            crate::HHDM_PHYSICAL_OFFSET,
            0x100000000,
            flags,
        );

        let kernel_phys = &__kernel_phys_start as *const _ as usize;
        let kernel_phys_end = &__kernel_phys_end as *const _ as usize;
        let kernel_virt = &__kernel_virt_start as *const _ as usize;
        let kernel_size = kernel_phys_end - kernel_phys;

        map_range(&mut off, l0, kernel_phys, kernel_virt, kernel_size, flags);

        let boot_phys = &__boot_start as *const _ as usize;
        let boot_phys_end = &__boot_end as *const _ as usize;
        let boot_size = boot_phys_end - boot_phys;
        map_range(&mut off, l0, boot_phys, boot_phys, boot_size, flags);

        map_range(
            &mut off,
            l0,
            PERIPHERAL_BASE,
            PERIPHERAL_BASE,
            0x200_0000,
            Arch::PAGE_FLAG_DEVICE,
        );

        map_range(
            &mut off,
            l0,
            dtb_ptr as usize,
            dtb_ptr as usize,
            32 * 1024 * 1024,
            flags,
        );

        let stack_bottom = &__boot_stack_bottom as *const _ as usize;
        let stack_top = &__boot_stack_top as *const _ as usize;
        let stack_size = stack_top - stack_bottom;
        map_range(&mut off, l0, stack_bottom, stack_bottom, stack_size, flags);

        const MCI: usize = (1 << 0) | (1 << 2) | (1 << 12);
        const TCR0: usize =
            ((64 - 48) << 0) | (0b01 << 8) | (0b01 << 10) | (0b11 << 12) | (0b00 << 14);
        const TCR1: usize =
            ((64 - 48) << 16) | (0b01 << 24) | (0b01 << 26) | (0b11 << 28) | (0b10 << 30);

        asm!(
            "mov x19, {dtb_ptr}",

            // Disable MMU
            "mrs    x0, sctlr_el1",
            "bic    x0, x0, 1",
            "msr    sctlr_el1, x0",
            "isb",

            // Install EL1 page tables
            "msr    mair_el1,   {mair}",
            "msr    tcr_el1,    {tcr}",
            "msr    ttbr0_el1,  {ttbr0}",
            "msr    ttbr1_el1,  {ttbr1}",

            // Clear TLB
            "dsb    ishst",
            "tlbi   vmalle1",
            "dsb    ish",
            "isb",

            // Zero the EL2 -> EL1 timer offset
            "msr    cntvoff_el2, xzr",
            "isb",

            // Configure HCR_EL2: un-trap IRQ/FIQ + EL1‑AArch64
            "mrs    x0, hcr_el2",
            "bic    x0, x0, {hcr_clear}",
            "orr    x0, x0, {hcr_set}",
            "msr    hcr_el2, x0",
            "isb",

            // Set up stack
            "ldr    x0, =__stack_top",
            "msr    sp_el1, x0",
            "ldr    x0, =__exception_vectors",
            "msr    vbar_el1, x0",

            // Enable MMU
            "mrs    x0, sctlr_el1",
            "orr    x0, x0, {mci}",
            "msr    sctlr_el1, x0",
            "isb",

            // Set up exception state & jump
            "mov    x0, x19",
            "msr    spsr_el2, {spsr}",
            "msr    SPSel, #1",
            "msr    elr_el2,  {entry}",
            "eret",

            mair        = in(reg) 0b11111111_00000000u64,
            tcr         = in(reg) (TCR0|TCR1) as u64,
            ttbr0       = in(reg) l0,
            ttbr1       = in(reg) l0,
            hcr_clear   = in(reg) ((1 << 8) | (1 << 9)) as u64,
            hcr_set     = in(reg) ((1 << 31) | (1 << 29)) as u64,
            mci         = in(reg) MCI,
            spsr        = in(reg) 0x3C5u64,
            dtb_ptr     = in(reg) dtb_ptr,
            entry       = in(reg) boot_higher_half,
            options(noreturn)
        );
    }
}

extern "C" fn boot_higher_half(dtb_ptr: *const u8) -> ! {
    unsafe {
        let fdt = Fdt::from_ptr(dtb_ptr).unwrap();
        let mut mem_map = MemMapEntries::new();

        let kernel_phys_start = &__kernel_phys_start as *const _ as usize;
        let kernel_phys_end = &__kernel_phys_end as *const _ as usize;
        let boot_phys_start = &__boot_start as *const _ as usize;
        let boot_phys_end = &__boot_end as *const _ as usize;

        let is_kernel = |p| (kernel_phys_start..kernel_phys_end).contains(&p);
        let is_boot = |p| (boot_phys_start..boot_phys_end).contains(&p);

        for region in fdt.memory().regions() {
            let mut start = (region.starting_address as usize).max(boot_phys_start);
            let end = start + region.size.unwrap_or(0);
            if start >= end {
                continue;
            }
            let mut page = start;
            while page < end {
                if is_kernel(page) {
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
                if is_boot(page) {
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

        crate::kernel_main()
    }
}

#[unsafe(link_section = ".boot")]
pub fn alloc_table(off: &mut usize) -> &'static mut Table {
    let table = unsafe { &mut *(*off as *mut Table) };
    let mut i = 0;
    while i < 512 {
        table.0[i] = 0;
        i += 1;
    }
    *off += size_of::<Table>();
    table
}

#[unsafe(link_section = ".boot")]
pub const fn l0_index(addr: usize) -> usize {
    (addr >> 39) & 0x1ff
}

#[unsafe(link_section = ".boot")]
pub const fn l1_index(addr: usize) -> usize {
    (addr >> 30) & 0x1ff
}

#[unsafe(link_section = ".boot")]
pub const fn l2_index(addr: usize) -> usize {
    (addr >> 21) & 0x1ff
}

#[unsafe(link_section = ".boot")]
pub const fn l3_index(addr: usize) -> usize {
    (addr >> 12) & 0x1ff
}

#[unsafe(link_section = ".boot")]
pub const fn set_entry(entry: &mut usize, addr: usize, flags: usize) {
    *entry = addr | flags;
}

#[unsafe(link_section = ".boot")]
pub const fn entry_addr(entry: usize) -> usize {
    entry & 0x0000FFFFFFFFF000
}

#[unsafe(link_section = ".boot")]
pub const fn entry_flags(entry: usize) -> usize {
    entry & Arch::PAGE_ENTRY_FLAGS_MASK
}

#[unsafe(link_section = ".boot")]
pub fn next_table(
    off: &mut usize,
    table: &mut Table,
    index: usize,
    insert_flags: usize,
) -> &'static mut Table {
    let entry = table.0[index];
    let table_addr = entry_addr(entry);
    if table_addr == 0 {
        let new_table = alloc_table(off);
        set_entry(
            &mut table.0[index],
            entry_addr(new_table as *const _ as usize),
            Arch::PAGE_FLAG_ACCESS
                | Arch::PAGE_FLAG_NON_BLOCK
                | Arch::PAGE_FLAG_PRESENT
                | insert_flags,
        );
        new_table
    } else {
        let table_addr = entry_addr(entry);
        let flags = entry_flags(entry) | insert_flags;
        set_entry(&mut table.0[index], table_addr, flags);
        unsafe { &mut *(table_addr as *mut _) }
    }
}

#[unsafe(link_section = ".boot")]
pub fn map_range(
    off: &mut usize,
    table: &mut Table,
    phys: usize,
    virt: usize,
    size: usize,
    flags: usize,
) {
    let mut mapped = 0;
    while mapped < size {
        let phys = phys + mapped;
        let virt = virt + mapped;
        let block_size = largest_aligned_block_size(phys, virt, size - mapped);
        match block_size {
            GB => map_to_1gib(off, table, phys, virt, flags),
            TWO_MB => map_to_2mib(off, table, phys, virt, flags),
            FOUR_KB => map_to_4kib(off, table, phys, virt, flags),
            _ => unreachable!(),
        }

        mapped += block_size;
    }
}

pub const KB: usize = 1024;
pub const FOUR_KB: usize = KB * 4;
pub const MB: usize = KB * 1024;
pub const TWO_MB: usize = MB * 2;
pub const GB: usize = MB * 1024;

#[unsafe(link_section = ".boot")]
fn is_aligned(x: usize, align: usize) -> bool {
    x % align == 0
}

#[unsafe(link_section = ".boot")]
fn largest_aligned_block_size(phys: usize, virt: usize, size: usize) -> usize {
    if is_aligned(phys, GB) && is_aligned(virt, GB) && size >= GB {
        GB
    } else if is_aligned(phys, TWO_MB) && is_aligned(virt, TWO_MB) && size >= TWO_MB {
        TWO_MB
    } else {
        FOUR_KB
    }
}

#[unsafe(link_section = ".boot")]
fn map_to_1gib(off: &mut usize, table: &mut Table, phys: usize, virt: usize, flags: usize) {
    let flags = flags & !Arch::PAGE_FLAG_NON_BLOCK;
    let l1 = next_table(off, table, l0_index(virt), 0);
    let idx = l1_index(virt);
    set_entry(&mut l1.0[idx], phys, flags);
}

#[unsafe(link_section = ".boot")]
fn map_to_2mib(off: &mut usize, table: &mut Table, phys: usize, virt: usize, flags: usize) {
    let flags = flags & !Arch::PAGE_FLAG_NON_BLOCK;
    let l1 = next_table(off, table, l0_index(virt), 0);
    let l2 = next_table(off, l1, l1_index(virt), 0);
    let idx = l2_index(virt);
    set_entry(&mut l2.0[idx], phys, flags);
}

#[unsafe(link_section = ".boot")]
fn map_to_4kib(off: &mut usize, table: &mut Table, phys: usize, virt: usize, flags: usize) {
    let flags = flags | Arch::PAGE_FLAG_NON_BLOCK;
    let l1 = next_table(off, table, l0_index(virt), 0);
    let l2 = next_table(off, l1, l1_index(virt), 0);
    let l3 = next_table(off, l2, l2_index(virt), 0);
    let idx = l3_index(virt);
    set_entry(&mut l3.0[idx], phys, flags);
}

// #[unsafe(link_section = ".boot")]
// pub fn map_range_el2(
//     off: &mut usize,
//     table: &mut Table,
//     phys: usize,
//     virt: usize,
//     size: usize,
//     flags: usize,
// ) {
//     let mut mapped = 0;
//     while mapped < size {
//         let phys = phys + mapped;
//         let virt = virt + mapped;
//         let insert_flags = Arch::PAGE_FLAG_TYPE;
//         let l1 = next_table(off, table, l0_index(virt), insert_flags);
//         let l2 = next_table(off, l1, l1_index(virt), insert_flags);
//         let l3 = next_table(off, l2, l2_index(virt), insert_flags);
//         let idx = l3_index(virt);
//         set_entry(&mut l3.0[idx], phys, flags);

//         mapped += Arch::PAGE_SIZE;
//     }
// }

#[unsafe(link_section = ".boot")]
#[inline(always)]
pub fn core_affinity() -> u8 {
    let mpidr: usize;
    unsafe {
        asm!("mrs {0}, mpidr_el1", out(reg) mpidr);
    }
    (mpidr & 0xff) as u8
}
