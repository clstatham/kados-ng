use crate::{
    arch::{Arch, ArchTrait},
    mem::{
        paging::allocator::KernelFrameAllocator,
        units::{FrameCount, PhysAddr},
    },
    syscall::errno::Errno,
};

pub struct Stack {
    base: PhysAddr,
}

impl Stack {
    pub fn new() -> Result<Self, Errno> {
        let base = unsafe {
            KernelFrameAllocator
                .allocate(FrameCount::new(16))
                .map_err(|_| Errno::ENOMEM)?
        };
        Ok(Self { base })
    }

    pub fn initial_top(&self) -> *mut u8 {
        unsafe {
            self.base
                .as_hhdm_virt()
                .as_raw_ptr_mut::<u8>()
                .add(Arch::PAGE_SIZE * 16)
        }
    }

    #[allow(clippy::len_without_is_empty)]
    pub const fn len(&self) -> usize {
        Arch::PAGE_SIZE * 16
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        if let Err(e) = KernelFrameAllocator.free(self.base, FrameCount::new(16)) {
            log::error!("Stack::drop(): {e}");
        }
    }
}
