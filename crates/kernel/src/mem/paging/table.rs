use core::{
    fmt::{Debug, Display},
    ops::{Index, IndexMut},
};

use derive_more::{BitAnd, BitOr, BitXor};

use crate::{
    arch::{Arch, ArchTrait},
    mem::{
        MemError,
        units::{PhysAddr, VirtAddr},
    },
    print, println,
};

use super::{
    allocator::KernelFrameAllocator,
    flush::{PageFlush, PageFlushAll},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum BlockSize {
    Page4KiB = Arch::PAGE_SHIFT,
    Block2MiB = Arch::PAGE_SHIFT + Arch::PAGE_ENTRY_SHIFT,
    Block1GiB = Arch::PAGE_SHIFT + Arch::PAGE_ENTRY_SHIFT * 2,
}

impl BlockSize {
    #[inline]
    pub const fn size(self) -> usize {
        1 << self as usize
    }

    #[inline]
    pub const fn mask(self) -> usize {
        self.size() - 1
    }

    pub const fn largest_aligned(page: VirtAddr, frame: PhysAddr, size: usize) -> Self {
        if page.is_aligned(BlockSize::Block1GiB.size())
            && frame.is_aligned(BlockSize::Block1GiB.size())
            && size >= BlockSize::Block1GiB.size()
        {
            BlockSize::Block1GiB
        } else if page.is_aligned(BlockSize::Block2MiB.size())
            && frame.is_aligned(BlockSize::Block2MiB.size())
            && size >= BlockSize::Block2MiB.size()
        {
            BlockSize::Block2MiB
        } else {
            BlockSize::Page4KiB
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(usize)]
pub enum PageTableLevel {
    Level1 = 1,
    Level2 = 2,
    Level3 = 3,
    Level4 = 4,
}

impl PageTableLevel {
    pub const fn next_down(self) -> Option<Self> {
        match self {
            Self::Level4 => Some(Self::Level3),
            Self::Level3 => Some(Self::Level2),
            Self::Level2 => Some(Self::Level1),
            Self::Level1 => None,
        }
    }

    pub const fn shift(self) -> usize {
        (self as usize - 1) * Arch::PAGE_ENTRY_SHIFT + Arch::PAGE_SHIFT
    }
}

#[repr(C, align(4096))]
pub struct RawPageTable {
    entries: [PageTableEntry; Arch::PAGE_ENTRIES],
}

impl RawPageTable {
    pub const EMPTY: Self = Self {
        entries: [PageTableEntry::UNUSED; Arch::PAGE_ENTRIES],
    };
}

impl Index<usize> for RawPageTable {
    type Output = PageTableEntry;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl IndexMut<usize> for RawPageTable {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableKind {
    User,
    Kernel,
}

pub struct PageTable {
    frame: PhysAddr,
    level: PageTableLevel,
    kind: TableKind,
}

impl PageTable {
    pub fn create(kind: TableKind) -> PageTable {
        let frame = unsafe { KernelFrameAllocator.allocate_one().expect("Out of memory") };
        PageTable {
            frame,
            level: PageTableLevel::Level4,
            kind,
        }
    }

    pub fn current(kind: TableKind) -> PageTable {
        unsafe {
            let frame = Arch::current_page_table(kind);
            PageTable {
                frame,
                level: PageTableLevel::Level4,
                kind,
            }
        }
    }

    pub fn phys_addr(&self) -> PhysAddr {
        self.frame
    }

    pub fn virt_addr(&self) -> VirtAddr {
        self.frame.as_hhdm_virt()
    }

    pub fn is_current(&self) -> bool {
        unsafe { self.frame == Arch::current_page_table(self.kind) }
    }

    pub unsafe fn make_current(&self) {
        unsafe {
            Arch::set_current_page_table(self.frame, self.kind);
        }
    }

    pub unsafe fn entry(&self, index: usize) -> PageTableEntry {
        unsafe {
            let addr = self
                .frame
                .add_bytes(index * size_of::<PageTableEntry>())
                .as_hhdm_virt();
            addr.read_volatile().unwrap()
        }
    }

    pub unsafe fn set_entry(&mut self, index: usize, entry: PageTableEntry) {
        unsafe {
            let addr = self
                .frame
                .add_bytes(index * size_of::<PageTableEntry>())
                .as_hhdm_virt();

            addr.write_volatile(entry).unwrap();
        }
    }

    pub fn next_table(&self, index: usize) -> Result<PageTable, MemError> {
        let next_level = self.level.next_down().ok_or(MemError::NoNextTable)?;
        let entry = unsafe { self.entry(index) };
        if entry.is_table() {
            Ok(PageTable {
                frame: entry.addr()?,
                level: next_level,
                kind: self.kind,
            })
        } else {
            Err(MemError::NoNextTable)
        }
    }

    pub fn next_table_create(
        &mut self,
        index: usize,
        insert_flags: PageFlags,
    ) -> Result<PageTable, MemError> {
        let next_level = self.level.next_down().ok_or(MemError::NoNextTable)?;
        let mut entry = unsafe { self.entry(index) };
        if entry.is_table() {
            entry.insert_flags(insert_flags);
            unsafe { self.set_entry(index, entry) };
        } else {
            let frame = unsafe { KernelFrameAllocator.allocate_one()? };
            unsafe { self.set_entry(index, PageTableEntry::new(frame, insert_flags)) };
        }

        let entry = unsafe { self.entry(index) };
        Ok(PageTable {
            frame: entry.addr()?,
            level: next_level,
            kind: self.kind,
        })
    }

    pub fn translate(&self, addr: VirtAddr) -> Result<PageTableEntry, MemError> {
        let p3 = self.next_table(addr.page_table_index(PageTableLevel::Level4))?;
        let p2 = p3.next_table(addr.page_table_index(PageTableLevel::Level3))?;
        let p1 = p2.next_table(addr.page_table_index(PageTableLevel::Level2))?;
        unsafe { Ok(p1.entry(addr.page_table_index(PageTableLevel::Level1))) }
    }

    pub fn map_to(
        &mut self,
        page: VirtAddr,
        frame: PhysAddr,
        block_size: BlockSize,
        flags: PageFlags,
    ) -> Result<PageFlush, MemError> {
        let insert_flags = PageFlags::new_table();
        match block_size {
            BlockSize::Block1GiB => self.map_to_1gib(page, frame, flags, insert_flags),
            BlockSize::Block2MiB => self.map_to_2mib(page, frame, flags, insert_flags),
            BlockSize::Page4KiB => self.map_to_4kib(page, frame, flags, insert_flags),
        }
    }

    pub fn kernel_map_range(
        &mut self,
        mut page: VirtAddr,
        mut frame: PhysAddr,
        mut size: usize,
        flags: PageFlags,
    ) -> Result<PageFlushAll, MemError> {
        while size != 0 {
            let block_size = BlockSize::largest_aligned(page, frame, size);
            let flush = self.map_to(page, frame, block_size, flags)?;
            unsafe { flush.ignore() };

            page = page.add_bytes(block_size.size());
            frame = frame.add_bytes(block_size.size());
            size -= block_size.size();
        }
        Ok(PageFlushAll)
    }

    fn map_to_1gib(
        &mut self,
        page: VirtAddr,
        frame: PhysAddr,
        flags: PageFlags,
        insert_flags: PageFlags,
    ) -> Result<PageFlush, MemError> {
        #[cfg(target_arch = "aarch64")]
        let flags = flags.with_flag(Arch::PAGE_FLAG_NON_BLOCK, false); // unset the "table" bit to make it a "block"

        let mut p3 =
            self.next_table_create(page.page_table_index(PageTableLevel::Level4), insert_flags)?;
        let idx = page.page_table_index(PageTableLevel::Level3);
        let entry = unsafe { p3.entry(idx) };
        if entry.is_unused() {
            unsafe {
                p3.set_entry(
                    idx,
                    PageTableEntry::new(frame, flags.with_flag(Arch::PAGE_FLAG_HUGE, true)),
                )
            };
        } else {
            return Err(MemError::PageAlreadyMapped(page, entry));
        }
        Ok(PageFlush::new(page))
    }

    fn map_to_2mib(
        &mut self,
        page: VirtAddr,
        frame: PhysAddr,
        flags: PageFlags,
        insert_flags: PageFlags,
    ) -> Result<PageFlush, MemError> {
        #[cfg(target_arch = "aarch64")]
        let flags = flags.with_flag(Arch::PAGE_FLAG_NON_BLOCK, false); // unset the "table" bit to make it a "block"

        let mut p3 =
            self.next_table_create(page.page_table_index(PageTableLevel::Level4), insert_flags)?;
        let mut p2 =
            p3.next_table_create(page.page_table_index(PageTableLevel::Level3), insert_flags)?;
        let idx = page.page_table_index(PageTableLevel::Level2);
        let entry = unsafe { p2.entry(idx) };

        if entry.is_unused() {
            unsafe {
                p2.set_entry(
                    idx,
                    PageTableEntry::new(frame, flags.with_flag(Arch::PAGE_FLAG_HUGE, true)),
                )
            };
        } else {
            return Err(MemError::PageAlreadyMapped(page, entry));
        }
        Ok(PageFlush::new(page))
    }

    fn map_to_4kib(
        &mut self,
        page: VirtAddr,
        frame: PhysAddr,
        flags: PageFlags,
        insert_flags: PageFlags,
    ) -> Result<PageFlush, MemError> {
        let mut p3 =
            self.next_table_create(page.page_table_index(PageTableLevel::Level4), insert_flags)?;
        let mut p2 =
            p3.next_table_create(page.page_table_index(PageTableLevel::Level3), insert_flags)?;
        let mut p1 =
            p2.next_table_create(page.page_table_index(PageTableLevel::Level2), insert_flags)?;
        let idx = page.page_table_index(PageTableLevel::Level1);
        let entry = unsafe { p1.entry(idx) };

        if entry.is_unused() {
            unsafe { p1.set_entry(idx, PageTableEntry::new(frame, flags)) };
        } else {
            return Err(MemError::PageAlreadyMapped(page, entry));
        }
        Ok(PageFlush::new(page))
    }

    pub fn dump(&self) {
        for entry_i in 0..Arch::PAGE_ENTRIES {
            let entry = unsafe { self.entry(entry_i) };
            if let Ok(addr) = entry.addr() {
                let flags = entry.flags();
                if !flags.is_present() {
                    continue;
                }
                for _ in 0..(4 - self.level as usize) {
                    print!("    ");
                }
                println!("{entry_i} = {addr} [{flags}]");
                if let Ok(next) = self.next_table(entry_i) {
                    next.dump();
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PageTableEntry(usize);

impl PageTableEntry {
    pub const UNUSED: Self = Self(0);

    pub fn new(address: PhysAddr, flags: PageFlags) -> Self {
        Self(
            (((address.value() >> Arch::PAGE_SHIFT) & Arch::PAGE_ENTRY_ADDR_MASK)
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
        self == &Self::UNUSED
    }

    pub fn addr(&self) -> Result<PhysAddr, MemError> {
        if self.flags().has_flag(Arch::PAGE_FLAG_HUGE) {
            return Err(MemError::HugePage);
        }
        let addr = PhysAddr::new(
            ((self.0 >> Arch::PAGE_ENTRY_ADDR_SHIFT) & Arch::PAGE_ENTRY_ADDR_MASK)
                << Arch::PAGE_SHIFT,
        )?;

        Ok(addr)
    }

    pub fn flags(&self) -> PageFlags {
        PageFlags::from_raw(self.raw() & Arch::PAGE_ENTRY_FLAGS_MASK)
    }

    pub fn is_table(&self) -> bool {
        if !self
            .addr()
            .is_ok_and(|addr| (Arch::PAGE_SIZE..VirtAddr::MAX_LOW.value()).contains(&addr.value()))
        {
            return false;
        }

        if !self.flags().is_present() || !self.flags().is_writable() {
            return false;
        }

        #[cfg(target_arch = "aarch64")]
        if !self.flags().has_flag(Arch::PAGE_FLAG_NON_BLOCK) {
            return false;
        }

        true
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
        Self(Arch::PAGE_FLAG_TABLE_DEFAULTS)
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

    #[cfg(target_arch = "aarch64")]
    pub fn new_device() -> Self {
        Self::from_raw(Arch::PAGE_FLAG_DEVICE)
    }

    pub const fn from_raw(raw: usize) -> Self {
        Self(raw)
    }

    pub const fn raw(&self) -> usize {
        self.0
    }

    pub const fn has_flag(&self, flag: usize) -> bool {
        self.0 & flag == flag && flag != 0
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
impl Display for PageFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let p = if self.is_present() { "P" } else { " " };
        let w = if self.is_writable() { "W" } else { " " };
        let e = if self.is_executable() { "E" } else { " " };
        write!(f, "{p}{w}{e}")
    }
}
