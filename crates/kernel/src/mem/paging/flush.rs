use crate::{
    arch::{Arch, ArchTrait},
    mem::units::VirtAddr,
};

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
