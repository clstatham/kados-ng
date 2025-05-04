use crate::{
    arch::{Arch, ArchTrait},
    mem::{
        MemError,
        units::{PhysAddr, VirtAddr},
    },
};

use super::{
    allocator::KernelFrameAllocator,
    table::{PageFlags, PageTable, PageTableEntry},
};

#[must_use = "Page table changes must be flushed"]
pub struct PageFlush(VirtAddr);

impl PageFlush {
    pub fn new(addr: VirtAddr) -> Self {
        Self(addr)
    }

    pub fn flush(self) {
        unsafe {
            Arch::invalidate_page(self.0);
        }
    }

    pub unsafe fn ignore(self) {
        #[allow(clippy::forget_non_drop)]
        core::mem::forget(self);
    }
}

pub struct Mapper<'a> {
    table: &'a mut PageTable,
}

impl<'a> Mapper<'a> {
    pub unsafe fn create() -> Result<Self, MemError> {
        let table_addr = unsafe { KernelFrameAllocator.allocate_one()? };
        Ok(Self {
            table: unsafe { &mut *table_addr.as_hhdm_virt().as_raw_ptr_mut() },
        })
    }

    pub unsafe fn current() -> Self {
        Self {
            table: PageTable::current(),
        }
    }

    pub fn is_current(&self) -> bool {
        self.table.phys_addr() == unsafe { Arch::current_page_table() }
    }

    #[inline(always)]
    pub unsafe fn make_current(&self) {
        unsafe {
            Arch::set_current_page_table(self.table.phys_addr());
        }
    }

    #[inline(always)]
    pub unsafe fn make_current_and_flush_tlb(&self) {
        unsafe {
            self.make_current();
            Arch::invalidate_all();
        }
    }

    pub fn table(&self) -> &PageTable {
        self.table
    }

    pub fn table_mut(&mut self) -> &mut PageTable {
        self.table
    }

    pub fn translate(&self, addr: VirtAddr) -> Result<PageTableEntry, MemError> {
        let p3 = self.table.next_table(addr.page_table_index(3))?;
        let p2 = p3.next_table(addr.page_table_index(2))?;
        let p1 = p2.next_table(addr.page_table_index(1))?;
        let entry = p1[addr.page_table_index(0)];
        Ok(entry)
    }

    pub unsafe fn map_hhdm(
        &mut self,
        phys: PhysAddr,
        flags: PageFlags,
    ) -> Result<PageFlush, MemError> {
        let virt = phys.as_hhdm_virt();
        unsafe { self.map_to(virt, phys, flags) }
    }

    pub unsafe fn map(&mut self, addr: VirtAddr, flags: PageFlags) -> Result<PageFlush, MemError> {
        unsafe {
            let phys = KernelFrameAllocator.allocate_one()?;
            self.map_to(addr, phys, flags)
        }
    }

    pub unsafe fn map_to(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageFlags,
    ) -> Result<PageFlush, MemError> {
        let insert_flags = PageFlags::new_table();

        let p3 = self
            .table
            .next_table_create(virt.page_table_index(3), insert_flags)?;
        let p2 = p3.next_table_create(virt.page_table_index(2), insert_flags)?;
        let p1 = p2.next_table_create(virt.page_table_index(1), insert_flags)?;
        let entry = &mut p1[virt.page_table_index(0)];
        *entry = PageTableEntry::new(phys.value(), flags);
        Ok(PageFlush::new(virt))
    }
}
