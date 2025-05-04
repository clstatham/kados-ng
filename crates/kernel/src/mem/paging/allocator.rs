use spin::Once;

use crate::{
    arch::{Arch, ArchTrait},
    mem::{
        MemError,
        units::{FrameCount, PhysAddr},
    },
    sync::IrqMutex,
};

use super::MemMapEntry;

static KERNEL_FRAME_ALLOCATOR: Once<IrqMutex<BumpFrameAllocator>> = Once::new();

pub fn add_kernel_frames(areas: &'static [MemMapEntry]) {
    KERNEL_FRAME_ALLOCATOR.call_once(|| IrqMutex::new(BumpFrameAllocator::new(areas)));
}

pub trait FrameAllocator {
    unsafe fn allocate(&mut self, count: FrameCount) -> Result<PhysAddr, MemError>;
    unsafe fn allocate_one(&mut self) -> Result<PhysAddr, MemError> {
        unsafe { self.allocate(FrameCount::ONE) }
    }
}

pub struct KernelFrameAllocator;

impl FrameAllocator for KernelFrameAllocator {
    unsafe fn allocate(&mut self, count: FrameCount) -> Result<PhysAddr, MemError> {
        unsafe { KERNEL_FRAME_ALLOCATOR.get().unwrap().lock().allocate(count) }
    }
}

pub struct BumpFrameAllocator {
    areas: &'static [MemMapEntry],
    bump: usize,
}

impl BumpFrameAllocator {
    pub fn new(areas: &'static [MemMapEntry]) -> Self {
        Self { areas, bump: 0 }
    }
}

impl FrameAllocator for BumpFrameAllocator {
    unsafe fn allocate(&mut self, count: FrameCount) -> Result<PhysAddr, MemError> {
        let size_bytes = count.to_bytes();

        let block = loop {
            let area = self.areas.first().ok_or(MemError::OutOfMemory)?;
            let offset = self.bump;
            if area.size.to_bytes() - self.bump < size_bytes {
                self.areas = &self.areas[1..];
                continue;
            }
            self.bump += size_bytes;
            break area.base.add(offset);
        };

        // important to zero out the memory!
        unsafe { block.as_hhdm_virt().fill(0, size_bytes) }?;

        Ok(block)
    }
}

pub struct BuddySystemFrameAllocator {
    allocator: buddy_system_allocator::FrameAllocator,
}

impl BuddySystemFrameAllocator {
    pub const fn const_default() -> Self {
        Self {
            allocator: buddy_system_allocator::FrameAllocator::new(),
        }
    }

    pub fn new(areas: &'static [MemMapEntry]) -> Self {
        let mut allocator = buddy_system_allocator::FrameAllocator::new();
        for area in areas {
            let base = area.base.value() / Arch::PAGE_SIZE;
            allocator.add_frame(base, base + area.size.frame_count());
        }
        Self { allocator }
    }
}

impl FrameAllocator for BuddySystemFrameAllocator {
    unsafe fn allocate(&mut self, count: FrameCount) -> Result<PhysAddr, MemError> {
        if let Some(addr) = self.allocator.alloc(count.frame_count()) {
            let addr = PhysAddr::new_canonical(addr);
            unsafe { addr.as_hhdm_virt().fill(0, count.to_bytes()) }?;
            Ok(addr)
        } else {
            Err(MemError::OutOfMemory)
        }
    }
}
