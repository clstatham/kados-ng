#![no_std]
#![feature(linkage)]
#![allow(clippy::identity_op, clippy::missing_safety_doc)]

use core::{
    arch::{asm, naked_asm},
    panic::PanicInfo,
};

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
    unsafe static __kernel_virt_end: u8;

    unsafe fn boot_higher_half(dtb_ptr: *const u8) -> !;
}

const PAGE_SHIFT: usize = 12;

const PAGE_ENTRY_ADDR_WIDTH: usize = 40;

const PAGE_FLAG_PRESENT: usize = 1 << 0;

const PAGE_FLAG_NON_EXECUTABLE: usize = 0b11 << 53;

const PAGE_FLAG_NON_BLOCK: usize = 1 << 1;
const PAGE_FLAG_ACCESS: usize = 1 << 10;
const PAGE_FLAG_NORMAL: usize = 1 << 2;
const PAGE_FLAG_INNER_SHAREABLE: usize = 0b11 << 8;
const PAGE_FLAG_OUTER_SHAREABLE: usize = 0b10 << 8;

const PAGE_FLAG_DEVICE: usize = PAGE_FLAG_PRESENT
    | PAGE_FLAG_NON_BLOCK
    | PAGE_FLAG_ACCESS
    | (0 << 2) // AttrIdx 0
    | (0 << 6) // AP (RW, priv)
    | PAGE_FLAG_OUTER_SHAREABLE
    | PAGE_FLAG_NON_EXECUTABLE;

const PERIPHERAL_BASE: usize = 0xFE00_0000;

const PAGE_ENTRY_ADDR_SIZE: usize = 1 << PAGE_ENTRY_ADDR_WIDTH;
const PAGE_ENTRY_ADDR_MASK: usize = PAGE_ENTRY_ADDR_SIZE - 1;
const PAGE_ENTRY_FLAGS_MASK: usize = !(PAGE_ENTRY_ADDR_MASK << PAGE_SHIFT);

const HHDM_PHYSICAL_OFFSET: usize = 0xffff_8000_0000_0000;

#[repr(C, align(4096))]
pub struct Table([usize; 512]);

#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start(dtb_ptr: *const u8) -> ! {
    naked_asm!(
        "
        mov x19, x0
        ldr x1, =__boot_stack_top
        mov sp, x1

        mrs x1, MPIDR_EL1
        ands x1, x1, #0xff
        b.ne 3f
        
        msr daifset, #0b1111

        mrs x1, SCTLR_EL2
        bic x1, x1, #1
        msr SCTLR_EL2, x1
        isb
        
        mrs x1, SCTLR_EL1
        bic x1, x1, #1
        msr SCTLR_EL1, x1
        isb

        ldr x1, =__boot_bss
        ldr x2, =__boot_bss_end
        mov x3, xzr
    1:
        cmp x1, x2
        b.hs 2f
        str x3, [x1], #8
        b 1b
    2:

        mov x0, x19
        bl boot_el2
    
    3:
        dsb sy
        1: wfe
        b 1b
        ",
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn boot_el2(dtb_ptr: *const u8) -> ! {
    unsafe {
        // boot_uart_putc(b'A');

        let mut off = &__boot_table as *const _ as usize;

        let l0 = alloc_table(&mut off);

        let flags = PAGE_FLAG_ACCESS
            | PAGE_FLAG_INNER_SHAREABLE
            | PAGE_FLAG_NON_BLOCK
            | PAGE_FLAG_NORMAL
            | PAGE_FLAG_PRESENT;

        // boot_uart_putc(b'B');

        map_range(&mut off, l0, 0, HHDM_PHYSICAL_OFFSET, 0x100000000, flags);

        let kernel_phys = &__kernel_phys_start as *const _ as usize;
        let kernel_phys_end = &__kernel_phys_end as *const _ as usize;
        let kernel_virt = &__kernel_virt_start as *const _ as usize;
        let kernel_size = kernel_phys_end - kernel_phys;

        // boot_uart_putc(b'C');
        map_range(&mut off, l0, kernel_phys, kernel_virt, kernel_size, flags);

        let boot_phys = &__boot_start as *const _ as usize;
        let boot_phys_end = &__boot_end as *const _ as usize;
        let boot_size = boot_phys_end - boot_phys;

        // boot_uart_putc(b'D');
        map_range(&mut off, l0, boot_phys, boot_phys, boot_size, flags);

        // boot_uart_putc(b'E');
        map_range(
            &mut off,
            l0,
            PERIPHERAL_BASE,
            PERIPHERAL_BASE,
            0x200_0000,
            PAGE_FLAG_DEVICE,
        );

        // boot_uart_putc(b'F');
        map_range(
            &mut off,
            l0,
            dtb_ptr as usize,
            dtb_ptr as usize,
            32 * 1024 * 1024,
            flags,
        );

        const MCI: usize = (1 << 0) | (1 << 2) | (1 << 12);
        const TCR0: usize =
            ((64 - 48) << 0) | (0b01 << 8) | (0b01 << 10) | (0b11 << 12) | (0b00 << 14);
        const TCR1: usize =
            ((64 - 48) << 16) | (0b01 << 24) | (0b01 << 26) | (0b11 << 28) | (0b10 << 30);

        // boot_uart_putc(b'G');
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
            "msr    elr_el2, {entry}",

            "eret",

            mair        = in(reg) ((0xff << 8) | 0x00) as u64,
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

#[inline(always)]
pub fn alloc_table(off: &mut usize) -> &'static mut Table {
    let table = unsafe { &mut *(*off as *mut Table) };
    // memzero(table as *mut Table as *mut u8, size_of::<Table>());
    *off += size_of::<Table>();
    table
}

#[inline(always)]
pub fn memzero(ptr: *mut u8, size: usize) {
    unsafe {
        asm!(
            "mov x0, {start}",
            "mov x1, {end}",
            "mov x2, xzr",
            "1: cmp x0, x1",
            "b.hs 2f",
            "str x2, [x0], #8",
            "b 1b",
            "2:",
            start = in(reg) ptr as u64,
            end = in(reg) (ptr as u64 + size as u64),
            out("x0") _,
            out("x1") _,
            out("x2") _,
        )
    }
}

pub const fn l0_index(addr: usize) -> usize {
    (addr >> 39) & 0x1ff
}

pub const fn l1_index(addr: usize) -> usize {
    (addr >> 30) & 0x1ff
}

pub const fn l2_index(addr: usize) -> usize {
    (addr >> 21) & 0x1ff
}

pub const fn l3_index(addr: usize) -> usize {
    (addr >> 12) & 0x1ff
}

pub const fn set_entry(entry: &mut usize, addr: usize, flags: usize) {
    *entry = addr | flags;
}

pub const fn entry_addr(entry: usize) -> usize {
    entry & 0x0000FFFFFFFFF000
}

pub const fn entry_flags(entry: usize) -> usize {
    entry & PAGE_ENTRY_FLAGS_MASK
}

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
            PAGE_FLAG_ACCESS | PAGE_FLAG_NON_BLOCK | PAGE_FLAG_PRESENT | insert_flags,
        );
        new_table
    } else {
        let table_addr = entry_addr(entry);
        let flags = entry_flags(entry) | insert_flags;
        set_entry(&mut table.0[index], table_addr, flags);
        unsafe { &mut *(table_addr as *mut _) }
    }
}

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

fn is_aligned(x: usize, align: usize) -> bool {
    x % align == 0
}

fn largest_aligned_block_size(phys: usize, virt: usize, size: usize) -> usize {
    if is_aligned(phys, GB) && is_aligned(virt, GB) && size >= GB {
        GB
    } else if is_aligned(phys, TWO_MB) && is_aligned(virt, TWO_MB) && size >= TWO_MB {
        TWO_MB
    } else {
        FOUR_KB
    }
}

fn map_to_1gib(off: &mut usize, table: &mut Table, phys: usize, virt: usize, flags: usize) {
    let flags = flags & !PAGE_FLAG_NON_BLOCK;
    let l1 = next_table(off, table, l0_index(virt), 0);
    let idx = l1_index(virt);
    set_entry(&mut l1.0[idx], phys, flags);
}

fn map_to_2mib(off: &mut usize, table: &mut Table, phys: usize, virt: usize, flags: usize) {
    let flags = flags & !PAGE_FLAG_NON_BLOCK;
    let l1 = next_table(off, table, l0_index(virt), 0);
    let l2 = next_table(off, l1, l1_index(virt), 0);
    let idx = l2_index(virt);
    set_entry(&mut l2.0[idx], phys, flags);
}

fn map_to_4kib(off: &mut usize, table: &mut Table, phys: usize, virt: usize, flags: usize) {
    let flags = flags | PAGE_FLAG_NON_BLOCK;
    let l1 = next_table(off, table, l0_index(virt), 0);
    let l2 = next_table(off, l1, l1_index(virt), 0);
    let l3 = next_table(off, l2, l2_index(virt), 0);
    let idx = l3_index(virt);
    set_entry(&mut l3.0[idx], phys, flags);
}

#[linkage = "weak"]
#[panic_handler]
pub fn panic(_: &PanicInfo) -> ! {
    loop {}
}
