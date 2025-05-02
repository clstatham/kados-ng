use buddy_system_allocator::LockedHeap;

use super::units::VirtAddr;

pub const KERNEL_HEAP_START: usize = 0xFFFF_FE80_0000_0000;
pub const KERNEL_HEAP_SIZE: usize = 1024 * 1024 * 64;

#[global_allocator]
static HEAP: LockedHeap<32> = LockedHeap::new();

pub unsafe fn add_to_heap(start: VirtAddr, end: VirtAddr) {
    unsafe {
        HEAP.lock().add_to_heap(start.value(), end.value());
    }
}
