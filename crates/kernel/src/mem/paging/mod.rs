use allocator::{FrameAllocator, KernelFrameAllocator};
use limine::memory_map::EntryType;
use mapper::Mapper;
use spin::Once;
use table::PageFlags;

use crate::{
    __rodata_end, __rodata_start, __text_end, __text_start, KERNEL_OFFSET, KERNEL_STACK_SIZE,
    arch::{Arch, ArchTrait},
    mem::units::VirtAddr,
};

use super::units::{FrameCount, PhysAddr};

pub mod allocator;
pub mod mapper;
pub mod table;

pub static MEM_MAP_ENTRIES: Once<MemMapEntries<64>> = Once::new();

#[derive(Clone, Copy)]
pub struct MemMapEntry {
    pub base: PhysAddr,
    pub size: FrameCount,
    pub kind: EntryType,
}

impl MemMapEntry {
    pub const EMPTY: Self = Self {
        base: PhysAddr::NULL,
        size: FrameCount::EMPTY,
        kind: EntryType::BAD_MEMORY,
    };
}

pub struct MemMapEntries<const N: usize> {
    pub usable_entries: [MemMapEntry; N],
    pub usable_entry_count: usize,
    pub identity_map_entries: [MemMapEntry; N],
    pub identity_map_entry_count: usize,
    pub kernel_entry: Once<MemMapEntry>,
}

impl<const N: usize> MemMapEntries<N> {
    pub fn new() -> Self {
        MemMapEntries {
            usable_entries: [MemMapEntry::EMPTY; N],
            usable_entry_count: 0,
            identity_map_entries: [MemMapEntry::EMPTY; N],
            identity_map_entry_count: 0,
            kernel_entry: Once::new(),
        }
    }

    pub fn push_usable(&mut self, entry: MemMapEntry) {
        self.usable_entries[self.usable_entry_count] = entry;
        self.usable_entry_count += 1;
    }

    pub fn push_identity_map(&mut self, entry: MemMapEntry) {
        self.identity_map_entries[self.identity_map_entry_count] = entry;
        self.identity_map_entry_count += 1;
    }

    pub fn set_kernel_entry(&mut self, entry: MemMapEntry) {
        self.kernel_entry.call_once(|| entry);
    }

    pub fn kernel_entry(&self) -> Option<&MemMapEntry> {
        self.kernel_entry.get()
    }

    pub fn usable_entries(&self) -> &[MemMapEntry] {
        &self.usable_entries[..self.usable_entry_count]
    }

    pub fn identity_map_entries(&self) -> &[MemMapEntry] {
        &self.identity_map_entries[..self.identity_map_entry_count]
    }
}

pub unsafe fn map_memory() -> ! {
    let mem_map = MEM_MAP_ENTRIES.get().unwrap();

    unsafe {
        let mut mapper = Mapper::create(KernelFrameAllocator).unwrap();
        log::debug!("Mapping free areas");
        for entry in mem_map
            .usable_entries()
            .iter()
            .chain(mem_map.identity_map_entries())
        {
            let base = entry.base;
            for frame_idx in 0..entry.size.frame_count() {
                let phys = PhysAddr::new_canonical(base.value() + frame_idx * Arch::PAGE_SIZE);
                let virt = VirtAddr::new_canonical(phys.value() + VirtAddr::MIN_HIGH.value());
                let flags = PageFlags::new_for_data_segment();
                let flush = mapper.map_to(virt, phys, flags).unwrap();
                flush.ignore();
            }
        }

        log::debug!("Mapping kernel");
        let kernel_entry = mem_map.kernel_entry().unwrap();
        let kernel_base = kernel_entry.base;
        let kernel_size = kernel_entry.size;
        for frame_idx in 0..kernel_size.frame_count() {
            let phys = PhysAddr::new_canonical(kernel_base.value() + frame_idx * Arch::PAGE_SIZE);
            let virt = VirtAddr::new_canonical(KERNEL_OFFSET + frame_idx * Arch::PAGE_SIZE);
            let flags = if (__text_start()..__text_end()).contains(&virt.value()) {
                PageFlags::new_for_text_segment()
            } else if (__rodata_start()..__rodata_end()).contains(&virt.value()) {
                PageFlags::new_for_rodata_segment()
            } else {
                PageFlags::new_for_data_segment()
            };
            let flush = mapper.map_to(virt, phys, flags).unwrap();
            flush.ignore();
        }

        log::debug!("New page table: {:?}", mapper.table().phys_addr());
        for i in 0..Arch::PAGE_ENTRIES {
            if let Ok(entry) = mapper.table().entry(i) {
                if entry.flags().is_present() {
                    log::debug!("{}: {} [{:?}]", i, entry.addr().unwrap(), entry.flags());
                }
            }
        }

        let stack_size = FrameCount::from_bytes(KERNEL_STACK_SIZE);
        let stack_base = KernelFrameAllocator.allocate(stack_size).unwrap();
        for frame_idx in 0..stack_size.frame_count() {
            let phys = PhysAddr::new_canonical(stack_base.value() + frame_idx * Arch::PAGE_SIZE);
            let virt =
                VirtAddr::new_canonical(VirtAddr::MIN_HIGH.value() + frame_idx * Arch::PAGE_SIZE);
            let flags = PageFlags::new_for_data_segment();
            let flush = mapper.map_to(virt, phys, flags).unwrap();
            flush.ignore();
        }
        let stack_top =
            (stack_base.add(stack_size.to_bytes())).value() + VirtAddr::MIN_HIGH.value();

        mapper.make_current();

        Arch::init_mem();

        Arch::set_stack_pointer(
            VirtAddr::new_canonical(stack_top),
            crate::kernel_main_post_paging as *const () as usize,
        )
    }
}
