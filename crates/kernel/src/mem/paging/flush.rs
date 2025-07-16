use crate::{
    arch::{Arch, Architecture},
    mem::units::VirtAddr,
};

/// A pending page flush operation for a specific virtual address.
///
/// Changes to page tables may not be applied immediately.
/// This struct represents a pending flush operation that must be executed
/// to ensure that the changes take effect and the CPU sees the updated page table entries.
///
/// Note that, unlike some other Rust OSes, this does not automatically flush the TLB on drop,
/// and therefore is marked as `#[must_use]`.
///
/// Internally, this uses architecture-specific assembly instructions to invalidate the TLB entry for the specified virtual address.
#[must_use = "Page table changes must be flushed"]
pub struct PageFlush(pub VirtAddr);

impl PageFlush {
    pub fn new(addr: VirtAddr) -> Self {
        Self(addr)
    }

    pub fn flush(self) {
        unsafe {
            Arch::invalidate_page(self.0);
        }
    }

    pub unsafe fn ignore(self) {
        #[allow(clippy::forget_non_drop)]
        core::mem::forget(self);
    }
}

/// A pending flush operation for all pages in the current page table.
///
/// Note that, unlike some other Rust OSes, this does not automatically flush the TLB on drop,
/// and therefore is marked as `#[must_use]`.
///
/// See also: [`PageFlush`].
#[must_use = "Page table changes must be flushed"]
pub struct PageFlushAll;

impl PageFlushAll {
    pub fn flush(self) {
        unsafe {
            Arch::invalidate_all();
        }
    }

    pub unsafe fn ignore(self) {
        #[allow(clippy::forget_non_drop)]
        core::mem::forget(self);
    }
}

/// A range of virtual addresses that need to be flushed from the TLB.
///
/// Note that, unlike some other Rust OSes, this does not automatically flush the TLB on drop,
/// and therefore is marked as `#[must_use]`.
///
/// See also: [`PageFlush`].
#[must_use = "Page table changes must be flushed"]
pub struct PageFlushRange {
    pub start: VirtAddr,
    pub end: VirtAddr,
}

impl PageFlushRange {
    pub fn new(start: VirtAddr, end: VirtAddr) -> Self {
        Self { start, end }
    }

    pub fn flush(self) {
        unsafe {
            let mut page = self.start.align_down(Arch::PAGE_SIZE);
            let end = self.end.align_up(Arch::PAGE_SIZE);
            while page < end {
                Arch::invalidate_page(page);
                page = page.add_bytes(Arch::PAGE_SIZE);
            }
        }
    }

    pub unsafe fn ignore(self) {
        #[allow(clippy::forget_non_drop)]
        core::mem::forget(self);
    }
}
