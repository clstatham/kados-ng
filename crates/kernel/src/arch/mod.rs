#[cfg(target_arch = "aarch64")]
pub mod aarch64;

#[cfg(target_arch = "aarch64")]
pub use self::aarch64::AArch64 as Arch;
#[cfg(target_arch = "aarch64")]
pub use self::aarch64::*;

pub mod driver;

use crate::{
    irq::IrqChip,
    mem::{
        paging::table::{PageTable, TableKind},
        units::{PhysAddr, VirtAddr},
    },
};

/// The Architecture trait defines the architecture-specific constants and methods
/// that are used throughout the kernel.
///
/// It provides a common interface for different architectures, allowing the kernel
/// to be portable and architecture-agnostic.
///
/// Each architecture must implement this trait to provide the necessary functionality
/// and constants specific to that architecture.
pub trait Architecture {
    /* Implementation-specific constants */

    /// The number of bits in a page table entry.
    ///
    /// This is typically 12 for 4KB pages, 21 for 2MB pages, and 30 for 1GB pages.
    const PAGE_SHIFT: usize;

    /// The number of bits in a page table entry index.
    ///
    /// This is typically 9 for 4KB pages, 12 for 2MB pages, and 21 for 1GB pages.
    /// This is the number of bits used to index into a page table entry.
    const PAGE_ENTRY_SHIFT: usize;

    /// The number of levels in the page table hierarchy.
    const PAGE_LEVELS: usize;

    /// The number of bits in the page table entry address.
    ///
    /// This is typically 40 for 64-bit architectures.
    /// This is the number of bits used to represent the address of a page table entry.
    const PAGE_ENTRY_ADDR_WIDTH: usize;

    /// The default flags for a regular page.
    /// This is typically the flags used for a page that is not a table or block,
    /// and is not device memory.
    const PAGE_FLAG_PAGE_DEFAULTS: usize;

    /// The default flags for a page table.
    const PAGE_FLAG_TABLE_DEFAULTS: usize;

    /// The "present" flag for a page table entry.
    ///
    /// This flag indicates whether the page is present in memory.
    const PAGE_FLAG_PRESENT: usize;

    /// The "read-only" flag for a page table entry.
    ///
    /// Implementations typically use either the "read-only" or "read-write" flag.
    const PAGE_FLAG_READONLY: usize;

    /// The "read-write" flag for a page table entry.
    ///
    /// Implementations typically use either the "read-only" or "read-write" flag.
    const PAGE_FLAG_READWRITE: usize;

    /// The "user" flag for a page table entry.
    ///
    /// This flag indicates whether the page is accessible to user mode or not.
    const PAGE_FLAG_USER: usize;

    /// The "executable" flag for a page table entry.
    ///
    /// Implementations typically use either the "executable" or "non-executable" flag.
    const PAGE_FLAG_EXECUTABLE: usize;

    /// The "non-executable" flag for a page table entry.
    ///
    /// Implementations typically use either the "executable" or "non-executable" flag.
    const PAGE_FLAG_NON_EXECUTABLE: usize;

    /// The "global" flag for a page table entry.
    ///
    /// Implementations typically use either the "global" or "non-global" flag.
    const PAGE_FLAG_GLOBAL: usize;

    /// The "non-global" flag for a page table entry.
    ///
    /// Implementations typically use either the "global" or "non-global" flag.
    const PAGE_FLAG_NON_GLOBAL: usize;

    /// The "huge" flag for a page table entry.
    ///
    /// This flag indicates whether the page covers a large range of memory.
    /// This is typically used for large pages (e.g., 2MB or 1GB pages).
    const PAGE_FLAG_HUGE: usize;

    /* Derived constants */

    /// The size of a page in bytes.
    ///
    /// This is typically 4096 bytes (4KB) for most architectures.
    const PAGE_SIZE: usize = 1 << Self::PAGE_SHIFT;

    /// The mask used to extract the offset within a page.
    ///
    /// This is typically 0xFFF for 4KB pages, 0x3FFFFF for 2MB pages, and 0x7FFFFFFF for 1GB pages.
    const PAGE_OFFSET_MASK: usize = Self::PAGE_SIZE - 1;

    /// The number of bits used to represent the address of a page table entry.
    ///
    /// This is typically 12 for 4KB pages, 21 for 2MB pages, and 30 for 1GB pages.
    const PAGE_ENTRY_ADDR_SHIFT: usize = Self::PAGE_SHIFT;

    /// The number of entries in a page table.
    ///
    /// This is typically 512 for 4KB pages, 2048 for 2MB pages, and 4096 for 1GB pages.
    const PAGE_ENTRIES: usize = 1 << Self::PAGE_ENTRY_SHIFT;

    /// The mask used to extract the index of a page table entry.
    ///
    /// This is typically 0x1FF for 4KB pages, 0x7FF for 2MB pages, and 0x1FFF for 1GB pages.
    const PAGE_ENTRY_MASK: usize = Self::PAGE_ENTRIES - 1;

    /// The size of a page table entry in bytes.
    ///
    /// This is typically 8 bytes for 64-bit architectures.
    const PAGE_ENTRY_SIZE: usize = 1 << (Self::PAGE_SHIFT - Self::PAGE_ENTRY_SHIFT);

    /// The size of a page table entry's address in bytes.
    ///
    /// This is typically `1 << 40`, or `0x1_0000_0000_0000` for 64-bit architectures.
    const PAGE_ENTRY_ADDR_SIZE: usize = 1 << Self::PAGE_ENTRY_ADDR_WIDTH;

    /// The mask used to extract the address from a page table entry.
    ///
    /// This is typically `0xFFFF_FFFF_FFFF_F000` for 64-bit architectures.
    const PAGE_ENTRY_ADDR_MASK: usize = Self::PAGE_ENTRY_ADDR_SIZE - 1;

    /// The mask used to extract the flags from a page table entry.
    /// This is typically `0x0000_0000_0000_0FFF` for 64-bit architectures.
    const PAGE_ENTRY_FLAGS_MASK: usize =
        !(Self::PAGE_ENTRY_ADDR_MASK << Self::PAGE_ENTRY_ADDR_SHIFT);

    /* Initialization */

    /// Initializes the architecture-specific components of the kernel.
    ///
    /// This function is called early in the kernel's boot process to set up
    /// the architecture-specific components that are needed for the other
    /// initialization functions to work correctly.
    unsafe fn init_pre_kernel_main();

    /// Initializes the memory management system.
    unsafe fn init_mem(mapper: &mut PageTable);

    /// Initializes any architecture-specific drivers.
    unsafe fn init_drivers();

    /// Initializes architecture-specific interrupt components.
    unsafe fn init_interrupts();

    /// Initializes the architecture-specific CPU-local block.
    unsafe fn init_cpu_local_block();

    /// Initializes the architecture-specific system call interface.
    unsafe fn init_syscalls();

    /* Interrupts */

    /// Enables interrupts.
    unsafe fn enable_interrupts();

    /// Disables interrupts.
    unsafe fn disable_interrupts();

    /// Sets the interrupt enable state.
    unsafe fn set_interrupts_enabled(enable: bool) {
        unsafe {
            if enable {
                Self::enable_interrupts();
            } else {
                Self::disable_interrupts();
            }
        }
    }

    /// Checks if interrupts are enabled.
    unsafe fn interrupts_enabled() -> bool;

    /* Memory management */

    /// Invalidates a page in the TLB, allowing the next access to the page to
    /// reload the page table entry from memory.
    unsafe fn invalidate_page(addr: VirtAddr);

    /// Invalidates all pages in the TLB, allowing the next access to any page
    /// to reload the page table entry from memory.
    unsafe fn invalidate_all();

    /// Returns the current page table's physical address.
    unsafe fn current_page_table(kind: TableKind) -> PhysAddr;

    /// Sets the current page table to the specified physical address.
    unsafe fn set_current_page_table(addr: PhysAddr, kind: TableKind);

    /* CPU state */

    /// Returns the curernt stack pointer.
    fn stack_pointer() -> usize;

    /// Returns the current frame pointer (also known as the base pointer or link register).
    fn frame_pointer() -> usize;

    /// Returns the virtual address of the current CPU-local block.
    fn current_cpu_local_block() -> VirtAddr;

    /* Drivers */

    /// Initializes an appropriate IRQ chip based on the given compatible string.
    fn new_irq_chip(compatible: &str) -> Option<alloc::boxed::Box<dyn IrqChip>>;

    /* Misc */

    /// Resets the system immediately.
    fn emergency_reset() -> !;

    /// Exits the QEMU emulator with the specified exit code.
    ///
    /// Used for debugging and testing purposes.
    fn exit_qemu(code: u32) -> !;

    /// Halts the CPU until the next interrupt.
    fn halt();

    /// Performs a no-operation (NOP) instruction.
    fn nop();

    /// Triggers a breakpoint exception.
    fn breakpoint();

    /// Halts the CPU and enters an infinite loop.
    #[inline]
    fn hcf() -> ! {
        loop {
            Self::halt();
            Self::nop();
        }
    }

    /// Delays execution for at least the specified number of cycles.
    ///
    /// This just calls `nop()` in a loop, so the delay is not precise.
    #[inline]
    fn delay_cycles(cycles: usize) {
        for _ in 0..cycles {
            Self::nop();
        }
    }
}
