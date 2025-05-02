use crate::{
    arch::{Arch, ArchTrait},
    mem::{
        MemError,
        units::{PhysAddr, VirtAddr},
    },
};

use super::{
    allocator::FrameAllocator,
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

pub struct PageFlushAll;

impl PageFlushAll {
    pub unsafe fn ignore(self) {
        core::mem::forget(self);
    }
}

impl Drop for PageFlushAll {
    fn drop(&mut self) {
        unsafe {
            Arch::invalidate_all();
        }
    }
}

pub struct Mapper<A: FrameAllocator> {
    table_addr: PhysAddr,
    allocator: A,
}

impl<A: FrameAllocator> Mapper<A> {
    pub fn new(table_addr: PhysAddr, allocator: A) -> Self {
        Self {
            table_addr,
            allocator,
        }
    }

    pub unsafe fn create(mut allocator: A) -> Result<Self, MemError> {
        let table_addr = unsafe { allocator.allocate_one()? };
        Ok(Self::new(table_addr, allocator))
    }

    pub unsafe fn current(allocator: A) -> Self {
        let table_addr = unsafe { Arch::current_page_table() };
        Self::new(table_addr, allocator)
    }

    pub fn is_current(&self) -> bool {
        self.table().phys_addr() == unsafe { Arch::current_page_table() }
    }

    pub unsafe fn make_current(&self) {
        unsafe {
            Arch::set_current_page_table(self.table_addr);
        }
    }

    pub unsafe fn make_current_and_flush_tlb(&self) {
        unsafe {
            self.make_current();
            Arch::invalidate_all();
        }
    }

    pub fn table(&self) -> PageTable {
        PageTable::new(VirtAddr::NULL, self.table_addr, Arch::PAGE_LEVELS - 1)
    }

    fn visit<R>(
        &self,
        addr: VirtAddr,
        f: impl FnOnce(&mut PageTable, usize) -> R,
    ) -> Result<R, MemError> {
        let mut table = self.table();

        loop {
            let i = table.index_of(addr)?;
            if table.level() == 0 {
                return Ok(f(&mut table, i));
            } else {
                table = table.next(i)?;
            }
        }
    }

    pub fn translate(&self, addr: VirtAddr) -> Result<PageTableEntry, MemError> {
        let entry = self.visit(addr, |table, i| table.entry(i))??;
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
            let phys = self.allocator.allocate_one()?;
            self.map_to(addr, phys, flags)
        }
    }

    pub unsafe fn map_to(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageFlags,
    ) -> Result<PageFlush, MemError> {
        let entry = PageTableEntry::new(phys.value(), flags.raw());
        let mut table = self.table();

        loop {
            let i = table.index_of(virt)?;
            if table.level() == 0 {
                // log::trace!("Mapping {virt:?} => {phys:?} with {flags:?}");
                // let existing_entry = table.entry(i)?;
                // if existing_entry.raw() != 0 {
                //     log::warn!(
                //         "REMAPPING {:?} from {:?} to {:?}",
                //         virt,
                //         existing_entry.addr(),
                //         entry.addr()
                //     );
                // }
                table.set_entry(i, entry)?;
                return Ok(PageFlush::new(virt));
            } else {
                let next = match table.next(i) {
                    Ok(next) => next,
                    Err(MemError::PageNotPresent(_)) => {
                        let next_phys = unsafe { self.allocator.allocate_one()? };
                        table.set_entry(
                            i,
                            PageTableEntry::new(next_phys.value(), Arch::PAGE_FLAG_TABLE_DEFAULTS),
                        )?;
                        table.next(i)?
                    }
                    Err(e) => return Err(e),
                };
                table = next;
            }
        }
    }
}
