use allocator::{FrameAllocator, KernelFrameAllocator};
use limine::memory_map::EntryType;
use mapper::Mapper;
use spin::Once;
use table::PageFlags;

use crate::{
    KERNEL_OFFSET,
    arch::{Arch, ArchTrait},
    mem::units::VirtAddr,
};

use super::{
    HHDM_PHYSICAL_OFFSET,
    units::{FrameCount, PhysAddr},
};

pub mod allocator;
pub mod mapper;
pub mod table;

pub static MEM_MAP_ENTRIES: Once<MemMapEntries<32>> = Once::new();

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
                let flags = PageFlags::new_for_addr(virt);
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
            let flags = PageFlags::new_for_addr(virt);
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

        const MAIR_EL1: usize = 0xFF | (0x04 << 8); // WB/WA cacheable RAM | Device-NgNRE
        // Provide the MMU with its parameters
        core::arch::asm!(
            r###"
            msr  mair_el1, {}"###, in(reg) MAIR_EL1, options(nostack));

        mapper.make_current();

        HHDM_PHYSICAL_OFFSET.store(
            VirtAddr::MIN_HIGH.value(),
            core::sync::atomic::Ordering::SeqCst,
        );

        let stack_size = FrameCount::from_bytes(0x200000);
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

        core::arch::asm!("msr SPSel, #1", "mov sp, {}", in(reg) stack_top); // stack top

        Arch::invalidate_all();

        core::arch::asm!(
            "
        mov x29, xzr
        bl kernel_main_post_paging",
            options(noreturn)
        )
    }
}
