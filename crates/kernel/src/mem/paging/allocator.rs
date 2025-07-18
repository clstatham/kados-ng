use alloc::boxed::Box;
use spin::{Mutex, MutexGuard, Once};

use crate::{
    BootInfo,
    arch::{Arch, Architecture},
    mem::{
        MemError,
        units::{FrameCount, PhysAddr},
    },
};

use super::MemMapEntry;

static KERNEL_FRAME_ALLOCATOR: Once<Mutex<FrameAllocator>> = Once::new();

/// Initializes the global kernel frame allocator with the boot memory map.
pub fn init_kernel_frame_allocator(boot_info: &'static BootInfo) {
    KERNEL_FRAME_ALLOCATOR
        .call_once(|| Mutex::new(FrameAllocator::boot(boot_info.mem_map.usable_entries())));
}

/// Returns a guard to the global kernel frame allocator.
///
/// # Panics
///
/// This function will panic if the kernel frame allocator has not been initialized.
#[must_use]
pub fn kernel_frame_allocator<'a>() -> MutexGuard<'a, FrameAllocator> {
    KERNEL_FRAME_ALLOCATOR
        .get()
        .expect("kernel frame allocator not initialized")
        .lock()
}

/// The frame allocator used by the kernel.
///
/// Pre-heap, it uses a bump allocator that allocates frames from the boot memory map.
/// Post-heap, it uses a buddy system allocator that manages frames more efficiently.
pub enum FrameAllocator {
    Boot(BumpFrameAllocator),
    PostHeap(Box<BuddySystemFrameAllocator>),
}

impl FrameAllocator {
    /// Creates a new frame allocator using the boot memory map.
    #[must_use]
    pub fn boot(areas: &'static [MemMapEntry]) -> Self {
        Self::Boot(BumpFrameAllocator::new(areas))
    }

    /// Converts the boot-phase bump allocator to a post-heap buddy system allocator.
    ///
    /// The buddy system allocator will inherit the frames that were allocated during the boot phase,
    /// as well as any remaining free frames.
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
            let first_base = first_free_area.base.add_bytes(bump.bump);
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

    /// Allocates a number of frames.
    pub unsafe fn allocate(&mut self, count: FrameCount) -> Result<PhysAddr, MemError> {
        match self {
            Self::Boot(bump) => unsafe { bump.allocate(count) },
            Self::PostHeap(buddy) => unsafe { buddy.allocate(count) },
        }
    }

    /// Frees a range of frames.
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

    /// Returns the number of frames currently allocated.
    /// Only returns a value for the bump allocator, as the buddy system allocator does not track usage.
    #[must_use]
    pub fn usage(&self) -> Option<FrameCount> {
        match self {
            Self::Boot(bump) => Some(bump.usage()),
            Self::PostHeap(_) => None,
        }
    }
}

/// A handle to the global kernel frame allocator.
pub struct KernelFrameAllocator;

impl KernelFrameAllocator {
    /// Allocates a number of frames from the global kernel frame allocator.
    pub unsafe fn allocate(&mut self, count: FrameCount) -> Result<PhysAddr, MemError> {
        unsafe { kernel_frame_allocator().allocate(count) }
    }

    /// Allocates a single frame from the global kernel frame allocator.
    pub unsafe fn allocate_one(&mut self) -> Result<PhysAddr, MemError> {
        unsafe { self.allocate(FrameCount::new(1)) }
    }

    /// Frees a range of frames in the global kernel frame allocator.
    pub fn free(&mut self, start: PhysAddr, count: FrameCount) -> Result<(), MemError> {
        kernel_frame_allocator().free(start, count)
    }

    /// Returns the number of frames currently allocated in the global kernel frame allocator.
    /// Only returns a value for the bump allocator, as the buddy system allocator does not track usage.
    #[must_use]
    pub fn usage(&self) -> Option<FrameCount> {
        kernel_frame_allocator().usage()
    }
}

/// A bump allocator for frames of physical memory.
pub struct BumpFrameAllocator {
    original: &'static [MemMapEntry],
    areas: &'static [MemMapEntry],
    bump: usize,
}

impl BumpFrameAllocator {
    /// Creates a new bump frame allocator with the given memory map entries for usable memory.
    #[must_use]
    pub fn new(areas: &'static [MemMapEntry]) -> Self {
        Self {
            original: areas,
            areas,
            bump: 0,
        }
    }

    /// Allocates a number of frames from the bump allocator.
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
            break area.base.add_bytes(offset);
        };

        unsafe {
            block.as_hhdm_virt().fill(0, size_bytes)?;
        }

        Ok(block)
    }

    /// Returns the number of frames currently allocated in the bump allocator.
    #[must_use]
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

/// A buddy system allocator for frames of physical memory.
pub struct BuddySystemFrameAllocator {
    allocator: buddy_system_allocator::FrameAllocator,
}

impl BuddySystemFrameAllocator {
    /// Creates a new buddy system frame allocator with no frames added.
    #[must_use]
    pub const fn const_default() -> Self {
        Self {
            allocator: buddy_system_allocator::FrameAllocator::new(),
        }
    }

    /// Creates a new buddy system frame allocator with the given memory map entries for usable memory.
    #[must_use]
    pub fn new(areas: &'static [MemMapEntry]) -> Self {
        let mut allocator = buddy_system_allocator::FrameAllocator::new();
        for area in areas {
            let base = area.base.value() / Arch::PAGE_SIZE;
            allocator.add_frame(base, base + area.size.frame_count());
        }
        Self { allocator }
    }

    /// Allocates a number of frames from the buddy system allocator.
    pub unsafe fn allocate(&mut self, count: FrameCount) -> Result<PhysAddr, MemError> {
        if let Some(frame) = self.allocator.alloc(count.frame_count()) {
            let addr = FrameCount::new(frame).to_bytes();
            let addr = PhysAddr::new_canonical(addr);
            unsafe { addr.as_hhdm_virt().fill(0, count.to_bytes())? };
            Ok(addr)
        } else {
            Err(MemError::OutOfMemory)
        }
    }

    /// Frees a range of frames in the buddy system allocator.
    pub fn free(&mut self, start: PhysAddr, count: FrameCount) -> Result<(), MemError> {
        self.allocator
            .dealloc(start.frame_index().frame_index(), count.frame_count());
        Ok(())
    }
}
