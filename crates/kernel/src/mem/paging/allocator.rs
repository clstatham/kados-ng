use alloc::boxed::Box;
use spin::Once;

use crate::{
    BOOT_INFO,
    arch::{Arch, ArchTrait},
    mem::{
        MemError,
        units::{FrameCount, PhysAddr},
    },
    sync::IrqMutex,
};

use super::MemMapEntry;

static KERNEL_FRAME_ALLOCATOR: Once<IrqMutex<FrameAllocator>> = Once::new();

pub fn init_kernel_frame_allocator() {
    let boot_info = BOOT_INFO.get().unwrap();
    KERNEL_FRAME_ALLOCATOR
        .call_once(|| IrqMutex::new(FrameAllocator::boot(boot_info.mem_map.usable_entries())));
}

pub fn kernel_frame_allocator() -> &'static IrqMutex<FrameAllocator> {
    KERNEL_FRAME_ALLOCATOR
        .get()
        .expect("kernel frame allocator not initialized")
}

pub enum FrameAllocator {
    Boot(BumpFrameAllocator),
    PostHeap(Box<BuddySystemFrameAllocator>),
}

impl FrameAllocator {
    pub fn boot(areas: &'static [MemMapEntry]) -> Self {
        Self::Boot(BumpFrameAllocator::new(areas))
    }

    pub fn convert_post_heap(&mut self) -> Result<(), MemError> {
        if let Self::Boot(bump) = self {
            let usage = bump.usage();
            log::info!(
                "Boot bump allocator permanently used {} frames ({} bytes)",
                usage.frame_count(),
                usage.to_bytes()
            );

            let mut buddy = Box::new(BuddySystemFrameAllocator::const_default());

            // inherit whatever's left of our first free area
            let first_free_area = bump.areas.first().ok_or(MemError::OutOfMemory)?;
            let first_base = first_free_area.base.add(bump.bump);
            let first_size = first_free_area.size.to_bytes() - bump.bump;
            let index = first_base.frame_index();
            let count = FrameCount::from_bytes(first_size);
            buddy.allocator.add_frame(
                index.frame_index(),
                index.frame_index() + count.frame_count(),
            );

            // inherit the rest
            for area in bump.areas.iter().skip(1) {
                let index = area.base.frame_index();
                let count = area.size.frame_count();
                buddy
                    .allocator
                    .add_frame(index.frame_index(), index.frame_index() + count);
            }

            *self = Self::PostHeap(buddy);
        }

        Ok(())
    }

    pub unsafe fn allocate(&mut self, count: FrameCount) -> Result<PhysAddr, MemError> {
        match self {
            Self::Boot(bump) => unsafe { bump.allocate(count) },
            Self::PostHeap(buddy) => unsafe { buddy.allocate(count) },
        }
    }

    pub fn free(&mut self, start: PhysAddr, count: FrameCount) -> Result<(), MemError> {
        match self {
            Self::Boot(_) => {
                log::debug!(
                    "free({start:?}, {count:?}) called on bump allocator, which does nothing"
                );
                Ok(())
            }
            Self::PostHeap(buddy) => buddy.free(start, count),
        }
    }

    pub fn usage(&self) -> Option<FrameCount> {
        match self {
            Self::Boot(bump) => Some(bump.usage()),
            Self::PostHeap(_) => None,
        }
    }
}

pub struct KernelFrameAllocator;

impl KernelFrameAllocator {
    pub unsafe fn allocate(&mut self, count: FrameCount) -> Result<PhysAddr, MemError> {
        unsafe { kernel_frame_allocator().lock().allocate(count) }
    }

    pub unsafe fn allocate_one(&mut self) -> Result<PhysAddr, MemError> {
        unsafe { self.allocate(FrameCount::new(1)) }
    }

    pub fn free(&mut self, start: PhysAddr, count: FrameCount) -> Result<(), MemError> {
        kernel_frame_allocator().lock().free(start, count)
    }

    pub fn usage(&self) -> Option<FrameCount> {
        kernel_frame_allocator().lock().usage()
    }
}

pub struct BumpFrameAllocator {
    original: &'static [MemMapEntry],
    areas: &'static [MemMapEntry],
    bump: usize,
}

impl BumpFrameAllocator {
    pub fn new(areas: &'static [MemMapEntry]) -> Self {
        Self {
            original: areas,
            areas,
            bump: 0,
        }
    }

    pub unsafe fn allocate(&mut self, count: FrameCount) -> Result<PhysAddr, MemError> {
        let size_bytes = count.to_bytes();

        let block = loop {
            let area = self.areas.first().ok_or(MemError::OutOfMemory)?;
            let offset = self.bump;
            if area.size.to_bytes() - self.bump < size_bytes {
                self.areas = &self.areas[1..];
                self.bump = 0;
                continue;
            }
            self.bump += size_bytes;
            break area.base.add(offset);
        };

        Ok(block)
    }

    pub fn usage(&self) -> FrameCount {
        let mut total = 0;
        let num_consumed = self.original.len() - self.areas.len();
        for area in &self.original[..num_consumed] {
            total += area.size.to_bytes();
        }
        total += self.bump;

        FrameCount::from_bytes(total)
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

    pub unsafe fn allocate(&mut self, count: FrameCount) -> Result<PhysAddr, MemError> {
        if let Some(addr) = self.allocator.alloc(count.frame_count()) {
            let addr = PhysAddr::new_canonical(addr);
            Ok(addr)
        } else {
            Err(MemError::OutOfMemory)
        }
    }

    pub fn free(&mut self, start: PhysAddr, count: FrameCount) -> Result<(), MemError> {
        self.allocator
            .dealloc(start.frame_index().frame_index(), count.frame_count());
        Ok(())
    }
}
