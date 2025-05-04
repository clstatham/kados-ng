use core::fmt::Debug;

use crate::{
    arch::{Arch, ArchTrait},
    mem::{
        MemError,
        units::{PhysAddr, VirtAddr},
    },
};

pub struct PageTable {
    base: VirtAddr,
    frame: PhysAddr,
    level: usize,
}

impl PageTable {
    pub fn new(base: VirtAddr, frame: PhysAddr, level: usize) -> Self {
        Self { base, frame, level }
    }

    pub fn current() -> Self {
        Self::new(
            VirtAddr::NULL,
            unsafe { Arch::current_page_table() },
            Arch::PAGE_LEVELS - 1,
        )
    }

    pub fn base(&self) -> VirtAddr {
        self.base
    }

    pub fn phys_addr(&self) -> PhysAddr {
        self.frame
    }

    pub fn level(&self) -> usize {
        self.level
    }

    pub fn virt_addr(&self) -> VirtAddr {
        self.frame.as_hhdm_virt()
    }

    pub fn entry_base(&self, i: usize) -> Result<VirtAddr, MemError> {
        if i < Arch::PAGE_ENTRIES {
            let level_shift = self.level * Arch::PAGE_ENTRY_SHIFT + Arch::PAGE_SHIFT;
            Ok(self.base.add(i << level_shift))
        } else {
            Err(MemError::InvalidPageTableIndex(i))
        }
    }

    pub fn entry_virt_addr(&self, i: usize) -> Result<VirtAddr, MemError> {
        if i < Arch::PAGE_ENTRIES {
            Ok(self.virt_addr().add(i * Arch::PAGE_ENTRY_SIZE))
        } else {
            Err(MemError::InvalidPageTableIndex(i))
        }
    }

    pub fn entry(&self, i: usize) -> Result<PageTableEntry, MemError> {
        let addr = self.entry_virt_addr(i)?;
        Ok(PageTableEntry::from_raw(unsafe { addr.read::<usize>()? }))
    }

    pub fn set_entry(&mut self, i: usize, entry: PageTableEntry) -> Result<(), MemError> {
        let addr = self.entry_virt_addr(i)?;
        unsafe {
            addr.write::<usize>(entry.raw())?;
        }
        Ok(())
    }

    pub fn index_of(&self, addr: VirtAddr) -> Result<usize, MemError> {
        let addr = VirtAddr::new_canonical(addr.value() & Arch::PAGE_ADDR_MASK);
        let level_shift = self.level * Arch::PAGE_ENTRY_SHIFT + Arch::PAGE_SHIFT;
        // let level_mask = (Arch::PAGE_ENTRIES << level_shift) - 1;
        let level_mask = Arch::PAGE_ENTRIES
            .wrapping_shl(level_shift as u32)
            .wrapping_sub(1);
        if addr >= self.base && addr <= self.base.add(level_mask) {
            Ok((addr.value() >> level_shift) & Arch::PAGE_ENTRY_MASK)
        } else {
            Err(MemError::NotPartOfTable(addr, self.phys_addr()))
        }
    }

    pub fn next(&self, i: usize) -> Result<Self, MemError> {
        if self.level == 0 {
            Err(MemError::NoNextTable)
        } else {
            Ok(PageTable::new(
                self.entry_base(i)?,
                self.entry(i)?.addr()?,
                self.level - 1,
            ))
        }
    }
}

#[derive(Clone, Copy)]
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
}

impl Debug for PageTableEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageTableEntry")
            .field("addr", &self.addr())
            .field("flags", &self.flags())
            .finish()
    }
}

#[derive(Clone, Copy)]
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
