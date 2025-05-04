use core::{
    fmt::Debug,
    ops::{Index, IndexMut},
};

use derive_more::{BitAnd, BitOr, BitXor};

use crate::{
    arch::{Arch, ArchTrait},
    mem::{
        MemError, hhdm_physical_offset,
        units::{PhysAddr, VirtAddr},
    },
};

use super::allocator::KernelFrameAllocator;

#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
#[derive(Clone)]
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; Arch::PAGE_ENTRIES],
}

impl PageTable {
    pub fn current<'a>() -> &'a mut Self {
        unsafe { &mut *Arch::current_page_table().as_hhdm_virt().as_raw_ptr_mut() }
    }

    pub fn phys_addr(&self) -> PhysAddr {
        PhysAddr::new_canonical(self as *const PageTable as usize - hhdm_physical_offset())
    }

    pub fn zero_out(&mut self) {
        for entry in self.entries.iter_mut() {
            *entry = PageTableEntry::from_raw(0);
        }
    }

    pub fn next_table(&self, index: usize) -> Result<&PageTable, MemError> {
        let ptr = self.entries[index].addr()?.as_hhdm_virt().as_raw_ptr();
        Ok(unsafe { &*ptr })
    }

    pub fn next_table_mut(&mut self, index: usize) -> Result<&mut PageTable, MemError> {
        let ptr = self.entries[index].addr()?.as_hhdm_virt().as_raw_ptr_mut();
        Ok(unsafe { &mut *ptr })
    }

    pub fn next_table_create(
        &mut self,
        index: usize,
        insert_flags: PageFlags,
    ) -> Result<&mut PageTable, MemError> {
        let entry = &mut self.entries[index];
        if entry.is_unused() {
            let addr = unsafe { KernelFrameAllocator.allocate_one()? };
            *entry = PageTableEntry::new(addr.value(), insert_flags);
        } else {
            entry.insert_flags(insert_flags);
        }

        self.next_table_mut(index)
    }
}

impl Index<usize> for PageTable {
    type Output = PageTableEntry;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl IndexMut<usize> for PageTable {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(usize);

impl PageTableEntry {
    pub fn new(address: usize, flags: PageFlags) -> Self {
        Self(
            (((address >> Arch::PAGE_SHIFT) & Arch::PAGE_ENTRY_ADDR_MASK)
                << Arch::PAGE_ENTRY_ADDR_SHIFT)
                | flags.raw(),
        )
    }

    pub fn from_raw(data: usize) -> Self {
        Self(data)
    }

    pub fn raw(&self) -> usize {
        self.0
    }

    pub fn is_unused(&self) -> bool {
        self.0 == 0
    }

    pub fn addr(&self) -> Result<PhysAddr, MemError> {
        let addr = PhysAddr::new(
            ((self.0 >> Arch::PAGE_ENTRY_ADDR_SHIFT) & Arch::PAGE_ENTRY_ADDR_MASK)
                << Arch::PAGE_SHIFT,
        )
        .unwrap();

        if self.flags().is_present() {
            Ok(addr)
        } else {
            Err(MemError::PageNotPresent(addr))
        }
    }

    pub fn flags(&self) -> PageFlags {
        PageFlags::from_raw(self.raw() & Arch::PAGE_ENTRY_FLAGS_MASK)
    }

    pub fn insert_flags(&mut self, flags: PageFlags) {
        self.0 |= flags.raw();
    }
}

impl Debug for PageTableEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageTableEntry")
            .field("addr", &self.addr())
            .field("flags", &self.flags())
            .finish()
    }
}

#[derive(Clone, Copy, BitOr, BitAnd, BitXor)]
pub struct PageFlags(usize);

impl PageFlags {
    pub const fn new() -> Self {
        Self(
            Arch::PAGE_FLAG_PAGE_DEFAULTS
                | Arch::PAGE_FLAG_READONLY
                | Arch::PAGE_FLAG_NON_EXECUTABLE
                | Arch::PAGE_FLAG_NON_GLOBAL,
        )
    }

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn new_table() -> Self {
        Self(Arch::PAGE_FLAG_TABLE_DEFAULTS | Arch::PAGE_FLAG_NON_GLOBAL)
    }

    pub const fn new_for_text_segment() -> Self {
        Self::new().executable()
    }

    pub fn new_for_rodata_segment() -> Self {
        Self::new()
    }

    pub fn new_for_data_segment() -> Self {
        Self::new().writable()
    }

    pub const fn from_raw(raw: usize) -> Self {
        Self(raw)
    }

    pub const fn raw(&self) -> usize {
        self.0
    }

    pub const fn has_flag(&self, flag: usize) -> bool {
        self.0 & flag == flag
    }

    pub const fn with_flag(&self, flag: usize, value: bool) -> Self {
        if value {
            Self(self.0 | flag)
        } else {
            Self(self.0 & !flag)
        }
    }

    pub const fn is_present(&self) -> bool {
        self.has_flag(Arch::PAGE_FLAG_PRESENT)
    }

    pub const fn present(self) -> Self {
        self.with_flag(Arch::PAGE_FLAG_PRESENT, true)
    }

    pub const fn is_executable(&self) -> bool {
        self.0 & (Arch::PAGE_FLAG_EXECUTABLE | Arch::PAGE_FLAG_NON_EXECUTABLE)
            == Arch::PAGE_FLAG_EXECUTABLE
    }

    pub const fn executable(self) -> Self {
        self.with_flag(Arch::PAGE_FLAG_EXECUTABLE, true)
            .with_flag(Arch::PAGE_FLAG_NON_EXECUTABLE, false)
    }

    pub const fn is_writable(&self) -> bool {
        self.0 & (Arch::PAGE_FLAG_READONLY | Arch::PAGE_FLAG_READWRITE) == Arch::PAGE_FLAG_READWRITE
    }

    pub const fn writable(self) -> Self {
        self.with_flag(Arch::PAGE_FLAG_READONLY | Arch::PAGE_FLAG_READWRITE, false)
            .with_flag(Arch::PAGE_FLAG_READWRITE, true)
    }
}

impl Debug for PageFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageFlags")
            .field("present", &self.is_present())
            .field("writable", &self.is_writable())
            .field("executable", &self.is_executable())
            .finish()
    }
}
