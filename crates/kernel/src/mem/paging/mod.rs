use allocator::KernelFrameAllocator;
use table::{BlockSize, PageFlags, PageTable, TableKind};

use crate::{
    __kernel_phys_end, __kernel_phys_start, __rodata_end, __rodata_start, __text_end, __text_start,
    BootInfo, KERNEL_OFFSET,
    arch::{Arch, Architecture},
    mem::{
        heap::{KERNEL_HEAP_SIZE, KERNEL_HEAP_START},
        units::VirtAddr,
    },
};

use super::units::{FrameCount, PhysAddr};

pub mod allocator;
pub mod flush;
pub mod table;

#[derive(Clone, Copy)]
pub struct MemMapEntry {
    pub base: PhysAddr,
    pub size: FrameCount,
}

impl MemMapEntry {
    pub const EMPTY: Self = Self {
        base: PhysAddr::NULL,
        size: FrameCount::EMPTY,
    };
}

pub struct MemMapEntries<const N: usize> {
    pub usable_entries: [MemMapEntry; N],
    pub usable_entry_count: usize,
}

impl<const N: usize> MemMapEntries<N> {
    pub fn new() -> Self {
        MemMapEntries {
            usable_entries: [MemMapEntry::EMPTY; N],
            usable_entry_count: 0,
        }
    }

    pub fn push_usable(&mut self, entry: MemMapEntry) {
        self.usable_entries[self.usable_entry_count] = entry;
        self.usable_entry_count += 1;
    }

    pub fn usable_entries(&self) -> &[MemMapEntry] {
        &self.usable_entries[..self.usable_entry_count]
    }
}

pub unsafe fn map_memory(boot_info: &BootInfo) {
    let mem_map = &boot_info.mem_map;

    let mut table = PageTable::create(TableKind::Kernel);
    log::debug!("mapping free areas");
    for entry in mem_map.usable_entries() {
        log::debug!(
            ">>> {} .. {} => {} .. {}",
            entry.base,
            entry.base.add_bytes(entry.size.to_bytes()),
            entry.base.as_hhdm_virt(),
            entry.base.as_hhdm_virt().add_bytes(entry.size.to_bytes()),
        );
        let phys = entry.base;
        let virt = phys.as_hhdm_virt();
        let flush = table
            .kernel_map_range(
                virt,
                phys,
                entry.size.to_bytes(),
                PageFlags::new_for_data_segment(),
            )
            .unwrap();
        unsafe { flush.ignore() }
    }

    log::debug!("mapping kernel");

    let kernel_base = __kernel_phys_start();
    let kernel_size = __kernel_phys_end() - kernel_base;
    let kernel_size = FrameCount::from_bytes(kernel_size);
    log::debug!(
        ">>> {} .. {} => {} .. {}",
        PhysAddr::new_canonical(kernel_base),
        PhysAddr::new_canonical(kernel_base + kernel_size.to_bytes()),
        VirtAddr::new_canonical(kernel_base + KERNEL_OFFSET),
        VirtAddr::new_canonical(kernel_base + KERNEL_OFFSET + kernel_size.to_bytes()),
    );
    for frame_idx in 0..kernel_size.frame_count() {
        let phys = PhysAddr::new_canonical(kernel_base + frame_idx * Arch::PAGE_SIZE);
        let virt = VirtAddr::new_canonical(KERNEL_OFFSET + frame_idx * Arch::PAGE_SIZE);

        let flags = if (__text_start()..__text_end()).contains(&virt.value()) {
            PageFlags::new_for_text_segment()
        } else if (__rodata_start()..__rodata_end()).contains(&virt.value()) {
            PageFlags::new_for_rodata_segment()
        } else {
            PageFlags::new_for_data_segment()
        };
        let flush = table
            .map_to(virt, phys, BlockSize::Page4KiB, flags)
            .unwrap();
        unsafe { flush.ignore() }

        let virt = phys.as_hhdm_virt();
        let flush = table
            .map_to(virt, phys, BlockSize::Page4KiB, flags)
            .unwrap();
        unsafe { flush.ignore() }
    }

    log::debug!("mapping heap");
    let frames = unsafe {
        KernelFrameAllocator
            .allocate(FrameCount::from_bytes(KERNEL_HEAP_SIZE))
            .unwrap()
    };
    log::debug!(
        ">>> {} .. {} => {} .. {}",
        frames,
        frames.add_bytes(KERNEL_HEAP_SIZE),
        VirtAddr::new_canonical(KERNEL_HEAP_START),
        VirtAddr::new_canonical(KERNEL_HEAP_START).add_bytes(KERNEL_HEAP_SIZE),
    );
    let flush = table
        .kernel_map_range(
            VirtAddr::new_canonical(KERNEL_HEAP_START),
            frames,
            KERNEL_HEAP_SIZE,
            PageFlags::new_for_data_segment(),
        )
        .unwrap();
    unsafe { flush.ignore() };

    unsafe {
        Arch::init_mem(&mut table);
        log::debug!("Making new page table current");
        table.make_current();
    }

    log::debug!("New page table: {:?}", table.phys_addr());
}
