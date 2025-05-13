use core::alloc::Layout;

use buddy_system_allocator::LockedHeap;

use crate::{
    arch::ArchTrait,
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
pub mod pcie;
pub mod usb;

pub const DMA_SIZE: usize = AArch64::PAGE_SIZE * 32;
static DMA_HEAP: LockedHeap<32> = LockedHeap::empty();

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
        )
    };
}

pub unsafe fn dma_alloc(bytes: usize) -> *mut u8 {
    DMA_HEAP
        .lock()
        .alloc(unsafe { Layout::from_size_align_unchecked(bytes, 16) })
        .unwrap()
        .as_ptr()
}
