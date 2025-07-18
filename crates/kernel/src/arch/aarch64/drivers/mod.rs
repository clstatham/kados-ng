use core::{alloc::Layout, ptr::NonNull};

use buddy_system_allocator::LockedHeap;

use crate::{
    arch::Architecture,
    mem::{
        paging::{
            allocator::KernelFrameAllocator,
            table::{BlockSize, PageFlags, PageTable},
        },
        units::FrameCount,
    },
};

use super::AArch64;

pub mod gpu;
pub mod mmio;

pub const DMA_SIZE: usize = AArch64::PAGE_SIZE * 32;
static DMA_HEAP: LockedHeap<32> = LockedHeap::empty();

/// Initializes the dedicated Direct Memory Access (DMA) heap.
///
/// # Panics
///
/// This function will panic if the memory allocation fails.
pub fn dma_init(mapper: &mut PageTable) {
    let base = unsafe {
        KernelFrameAllocator
            .allocate(FrameCount::from_bytes(DMA_SIZE))
            .unwrap()
    };

    unsafe {
        mapper
            .map_range_with_block_size(
                base.as_identity_virt(),
                base,
                DMA_SIZE,
                BlockSize::Page4KiB,
                PageFlags::new_for_data_segment(),
            )
            .unwrap()
            .ignore();
    };

    unsafe {
        DMA_HEAP.lock().add_to_heap(
            base.as_hhdm_virt().value(),
            base.as_hhdm_virt().add_bytes(DMA_SIZE).value(),
        );
    };
}

/// Allocates a zero-initialized object of type `T` from the DMA heap.
///
/// # Panics
///
/// This function will panic if the alignment of `T` is not a multiple of 16 or if the allocation fails.
pub fn dma_alloc<T>() -> *mut T {
    assert_eq!(align_of::<T>() % 16, 0);
    DMA_HEAP
        .lock()
        .alloc(Layout::new::<T>())
        .unwrap()
        .as_ptr()
        .cast()
}

/// Deallocates an object of type `T` from the DMA heap.
///
/// # Panics
///
/// This function will panic if the pointer is null or if the deallocation fails.
pub fn dma_free<T>(t: *mut T) {
    DMA_HEAP
        .lock()
        .dealloc(NonNull::new(t).unwrap().cast(), Layout::new::<T>());
}
