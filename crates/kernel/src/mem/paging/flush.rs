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

pub struct PageFlushAll;

impl PageFlushAll {
    pub unsafe fn ignore(self) {
        core::mem::forget(self);
    }
}

impl Drop for PageFlushAll {
    fn drop(&mut self) {
        unsafe {
            Arch::invalidate_all();
        }
    }
}
