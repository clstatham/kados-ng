use buddy_system_allocator::LockedHeap;

use crate::arch::{Arch, ArchTrait};

use super::{
    MemError,
    paging::{allocator::KernelFrameAllocator, mapper::Mapper, table::PageFlags},
    units::VirtAddr,
};

pub const KERNEL_HEAP_START: usize = 0xFFFF_FE80_0000_0000;
pub const KERNEL_HEAP_SIZE: usize = 1024 * 1024 * 64;

#[global_allocator]
static HEAP: LockedHeap<32> = LockedHeap::new();

pub unsafe fn init_heap() {
    unsafe {
        HEAP.lock().init(KERNEL_HEAP_START, KERNEL_HEAP_SIZE);
    }
}
