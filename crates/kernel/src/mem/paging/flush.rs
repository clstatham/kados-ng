use crate::{
    arch::{Arch, ArchTrait},
    mem::units::VirtAddr,
};

#[must_use = "Page table changes must be flushed"]
pub struct PageFlush(VirtAddr);

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
