use core::{
    fmt::{Debug, Display},
    ops::{Index, IndexMut},
};

use derive_more::{BitAnd, BitOr, BitXor};

use crate::{
    arch::{Arch, Architecture},
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

/// The size of a page table entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum BlockSize {
    Page4KiB = Arch::PAGE_SHIFT,
    Block2MiB = Arch::PAGE_SHIFT + Arch::PAGE_ENTRY_SHIFT,
    Block1GiB = Arch::PAGE_SHIFT + Arch::PAGE_ENTRY_SHIFT * 2,
}

impl BlockSize {
    /// Returns the size of the block in bytes.
    #[inline]
    #[must_use]
    pub const fn size(self) -> usize {
        1 << self as usize
    }

    /// Returns a bitmask for the block size.
    #[inline]
    #[must_use]
    pub const fn mask(self) -> usize {
        self.size() - 1
    }

    /// Returns the largest block size that can be used for the given page, frame, and size of the mapping in bytes.
    ///
    /// For example, if the page and frame are both aligned to 1 GiB, and the size is at least 1 GiB,
    /// it will return [`BlockSize::Block1GiB`].
    #[inline]
    #[must_use]
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

/// The level of a page table in the hierarchy.
///
/// A `Level4` table is the top-level table, while a `Level1` table is the bottom-level table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(usize)]
pub enum PageTableLevel {
    Level1 = 1,
    Level2 = 2,
    Level3 = 3,
    Level4 = 4,
}

impl PageTableLevel {
    /// Returns the next lower level of the page table, if applicable.
    #[must_use]
    pub const fn next_down(self) -> Option<Self> {
        match self {
            Self::Level4 => Some(Self::Level3),
            Self::Level3 => Some(Self::Level2),
            Self::Level2 => Some(Self::Level1),
            Self::Level1 => None,
        }
    }

    /// Returns the bit shift for the page table level.
    #[must_use]
    pub const fn shift(self) -> usize {
        (self as usize - 1) * Arch::PAGE_ENTRY_SHIFT + Arch::PAGE_SHIFT
    }
}

/// A raw, page-aligned array of page table entries.
/// These are usually transmuted from a raw pointer so that individual entries can be accessed
/// and modified directly.
#[repr(C, align(4096))]
pub struct RawPageTable {
    entries: [PageTableEntry; Arch::PAGE_ENTRIES],
}

impl RawPageTable {
    /// An empty page table, available as a constant for static initialization.
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

/// A marker for whether a page table is for user space or kernel space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableKind {
    User,
    Kernel,
}

/// A logical page table that can be used to manage memory mappings.
///
/// This differs from the [`RawPageTable`] in that it provides methods to create, modify, and traverse the page table hierarchy,
/// whereas the `RawPageTable` is a simple array of entries.
pub struct PageTable {
    frame: PhysAddr,
    level: PageTableLevel,
    kind: TableKind,
}

impl PageTable {
    /// Allocates a new level-4 page table using the global kernel frame allocator.
    ///
    /// # Panics
    ///
    /// Panics if the frame allocator runs out of memory.
    #[must_use]
    pub fn create(kind: TableKind) -> PageTable {
        let frame = unsafe { KernelFrameAllocator.allocate_one().expect("Out of memory") };
        PageTable {
            frame,
            level: PageTableLevel::Level4,
            kind,
        }
    }

    /// Returns the current page table of the given kind, as read by [`Arch::current_page_table`].
    #[must_use]
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

    /// Returns the physical address of the base of the page table.
    #[must_use]
    pub fn phys_addr(&self) -> PhysAddr {
        self.frame
    }

    /// Returns the virtual address of the base of the page table.
    #[must_use]
    pub fn virt_addr(&self) -> VirtAddr {
        self.frame.as_hhdm_virt()
    }

    /// Returns `true` if this page table is the current page table for the given kind,
    #[must_use]
    pub fn is_current(&self) -> bool {
        unsafe { self.frame == Arch::current_page_table(self.kind) }
    }

    /// Makes this page table the current page table for its kind.
    pub unsafe fn make_current(&self) {
        unsafe {
            Arch::set_current_page_table(self.frame, self.kind);
        }
    }

    /// Returns a copy of the page table entry at the given index.
    ///
    /// # Panics
    ///
    /// Panics if reading the entry fails.
    #[must_use]
    pub unsafe fn entry(&self, index: usize) -> PageTableEntry {
        unsafe {
            let addr = self
                .frame
                .add_bytes(index * size_of::<PageTableEntry>())
                .as_hhdm_virt();
            addr.read_volatile().unwrap()
        }
    }

    /// Sets the page table entry at the given index to the given entry.
    ///
    /// # Panics
    ///
    /// Panics if writing the entry fails.
    pub unsafe fn set_entry(&mut self, index: usize, entry: PageTableEntry) {
        unsafe {
            let addr = self
                .frame
                .add_bytes(index * size_of::<PageTableEntry>())
                .as_hhdm_virt();
            addr.write_volatile(entry).unwrap();
        }
    }

    /// Returns the next-down page table at the given entry index, if it exists and this is not a level-1 table.
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

    /// Creates and returns a new next-down page table at the given index, if it does not already exist;
    /// otherwise returns the existing one.
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

    /// Translates a virtual address to a level-1 page table entry, allowing access to the page's frame and flags.
    pub fn translate(&self, addr: VirtAddr) -> Result<PageTableEntry, MemError> {
        let p3 = self.next_table(addr.page_table_index(PageTableLevel::Level4))?;
        let p2 = p3.next_table(addr.page_table_index(PageTableLevel::Level3))?;
        let p1 = p2.next_table(addr.page_table_index(PageTableLevel::Level2))?;
        unsafe { Ok(p1.entry(addr.page_table_index(PageTableLevel::Level1))) }
    }

    /// Allows modification of a page table entry at the given virtual address.
    ///
    /// Returns a [`PageFlush`] that must be flushed after the modification.
    pub fn with_frame_mut<R>(
        &mut self,
        addr: VirtAddr,
        f: impl FnOnce(&mut PageTableEntry),
    ) -> Result<PageFlush, MemError> {
        let p3 = self.next_table(addr.page_table_index(PageTableLevel::Level4))?;
        let p2 = p3.next_table(addr.page_table_index(PageTableLevel::Level3))?;
        let mut p1 = p2.next_table(addr.page_table_index(PageTableLevel::Level2))?;
        let mut entry = unsafe { p1.entry(addr.page_table_index(PageTableLevel::Level1)) };
        f(&mut entry);
        unsafe {
            p1.set_entry(addr.page_table_index(PageTableLevel::Level1), entry);
        }
        Ok(PageFlush::new(addr))
    }

    /// Remaps a page to a new frame with the given block size and flags.
    ///
    /// This will NOT error if the page is already mapped, but will instead overwrite the existing mapping.
    /// For mapping with error on existing mapping, use [`PageTable::map_to`].
    pub fn remap_to(
        &mut self,
        page: VirtAddr,
        frame: PhysAddr,
        block_size: BlockSize,
        flags: PageFlags,
    ) -> Result<PageFlush, MemError> {
        let insert_flags = PageFlags::new_table();
        match block_size {
            BlockSize::Block1GiB => self.map_to_1gib(page, frame, flags, insert_flags, true),
            BlockSize::Block2MiB => self.map_to_2mib(page, frame, flags, insert_flags, true),
            BlockSize::Page4KiB => self.map_to_4kib(page, frame, flags, insert_flags, true),
        }
    }

    /// Maps a page to a frame with the given block size and flags.
    ///
    /// This will error if the page is already mapped. For remapping, use [`PageTable::remap_to`].
    pub fn map_to(
        &mut self,
        page: VirtAddr,
        frame: PhysAddr,
        block_size: BlockSize,
        flags: PageFlags,
    ) -> Result<PageFlush, MemError> {
        let insert_flags = PageFlags::new_table();
        match block_size {
            BlockSize::Block1GiB => self.map_to_1gib(page, frame, flags, insert_flags, false),
            BlockSize::Block2MiB => self.map_to_2mib(page, frame, flags, insert_flags, false),
            BlockSize::Page4KiB => self.map_to_4kib(page, frame, flags, insert_flags, false),
        }
    }

    /// Maps a range of pages to frames in the kernel address space.
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

    /// Maps a range of pages to frames with the given block size and flags.
    pub fn map_range_with_block_size(
        &mut self,
        mut page: VirtAddr,
        mut frame: PhysAddr,
        mut size: usize,
        block_size: BlockSize,
        flags: PageFlags,
    ) -> Result<PageFlushAll, MemError> {
        while size != 0 {
            let flush = self.map_to(page, frame, block_size, flags)?;
            unsafe { flush.ignore() };

            page = page.add_bytes(block_size.size());
            frame = frame.add_bytes(block_size.size());
            size -= block_size.size();
        }
        Ok(PageFlushAll)
    }

    /// Remaps a range of pages to frames in the kernel address space.
    pub fn kernel_remap_range(
        &mut self,
        mut page: VirtAddr,
        mut frame: PhysAddr,
        mut size: usize,
        flags: PageFlags,
    ) -> Result<PageFlushAll, MemError> {
        while size != 0 {
            let block_size = BlockSize::largest_aligned(page, frame, size);
            let flush = self.remap_to(page, frame, block_size, flags)?;
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
        remap: bool,
    ) -> Result<PageFlush, MemError> {
        #[cfg(target_arch = "aarch64")]
        let flags = flags.with_flag(Arch::PAGE_FLAG_NON_BLOCK, false); // unset the "table" bit to make it a "block"

        let mut p3 =
            self.next_table_create(page.page_table_index(PageTableLevel::Level4), insert_flags)?;
        let idx = page.page_table_index(PageTableLevel::Level3);
        let entry = unsafe { p3.entry(idx) };
        if entry.is_unused() || remap {
            unsafe {
                p3.set_entry(
                    idx,
                    PageTableEntry::new(frame, flags.with_flag(Arch::PAGE_FLAG_HUGE, true)),
                );
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
        remap: bool,
    ) -> Result<PageFlush, MemError> {
        #[cfg(target_arch = "aarch64")]
        let flags = flags.with_flag(Arch::PAGE_FLAG_NON_BLOCK, false); // unset the "table" bit to make it a "block"

        let mut p3 =
            self.next_table_create(page.page_table_index(PageTableLevel::Level4), insert_flags)?;
        let mut p2 =
            p3.next_table_create(page.page_table_index(PageTableLevel::Level3), insert_flags)?;
        let idx = page.page_table_index(PageTableLevel::Level2);
        let entry = unsafe { p2.entry(idx) };

        if entry.is_unused() || remap {
            unsafe {
                p2.set_entry(
                    idx,
                    PageTableEntry::new(frame, flags.with_flag(Arch::PAGE_FLAG_HUGE, true)),
                );
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
        remap: bool,
    ) -> Result<PageFlush, MemError> {
        let mut p3 =
            self.next_table_create(page.page_table_index(PageTableLevel::Level4), insert_flags)?;
        let mut p2 =
            p3.next_table_create(page.page_table_index(PageTableLevel::Level3), insert_flags)?;
        let mut p1 =
            p2.next_table_create(page.page_table_index(PageTableLevel::Level2), insert_flags)?;
        let idx = page.page_table_index(PageTableLevel::Level1);
        let entry = unsafe { p1.entry(idx) };

        if entry.is_unused() || remap {
            unsafe { p1.set_entry(idx, PageTableEntry::new(frame, flags)) };
        } else {
            return Err(MemError::PageAlreadyMapped(page, entry));
        }
        Ok(PageFlush::new(page))
    }

    /// Dumps the page table entries to the console, showing their addresses and flags.
    /// This is VERY verbose and should only be used for debugging purposes.
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

/// A single page table entry, representing a mapping from a virtual address to a physical address
/// with associated flags.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PageTableEntry(usize);

impl PageTableEntry {
    /// Creates a new unused page table entry.
    pub const UNUSED: Self = Self(0);

    /// Creates a new page table entry with the given physical address and flags.
    #[must_use]
    pub fn new(address: PhysAddr, flags: PageFlags) -> Self {
        Self(
            (((address.value() >> Arch::PAGE_SHIFT) & Arch::PAGE_ENTRY_ADDR_MASK)
                << Arch::PAGE_ENTRY_ADDR_SHIFT)
                | flags.raw(),
        )
    }

    /// Creates a new page table entry from a raw double word value.
    #[must_use]
    pub fn from_raw(data: usize) -> Self {
        Self(data)
    }

    /// Returns the raw value of the page table entry as an unsigned double word value.
    #[must_use]
    pub fn raw(&self) -> usize {
        self.0
    }

    /// Returns `true` if this page table entry is unused.
    #[must_use]
    pub fn is_unused(&self) -> bool {
        self == &Self::UNUSED
    }

    /// Returns the physical address of the page table entry.
    ///
    /// Errors if the entry is a huge page (1 GiB or 2 MiB).
    pub fn addr(&self) -> Result<PhysAddr, MemError> {
        if self.flags().has_flags(Arch::PAGE_FLAG_HUGE) {
            return Err(MemError::HugePage);
        }
        let addr = PhysAddr::new(
            ((self.0 >> Arch::PAGE_ENTRY_ADDR_SHIFT) & Arch::PAGE_ENTRY_ADDR_MASK)
                << Arch::PAGE_SHIFT,
        )?;

        Ok(addr)
    }

    /// Returns the flags of the page table entry.
    #[must_use]
    pub fn flags(&self) -> PageFlags {
        PageFlags::from_raw(self.raw() & Arch::PAGE_ENTRY_FLAGS_MASK)
    }

    /// Returns `true` if this page table entry is a valid page table.
    #[must_use]
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
        if !self.flags().has_flags(Arch::PAGE_FLAG_NON_BLOCK) {
            return false;
        }

        true
    }

    /// Inserts the given flags into the page table entry using a bitwise OR operation.
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

/// Flags for a page table entry, representing various properties of the page.
#[derive(Clone, Copy, BitOr, BitAnd, BitXor)]
pub struct PageFlags(usize);

impl PageFlags {
    /// Creates a new set of page flags with default values.
    #[must_use]
    pub const fn new() -> Self {
        Self(
            Arch::PAGE_FLAG_PAGE_DEFAULTS
                | Arch::PAGE_FLAG_READONLY
                | Arch::PAGE_FLAG_NON_EXECUTABLE
                | Arch::PAGE_FLAG_NON_GLOBAL,
        )
    }

    /// Creates an empty set of page flags, with no flags set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Creates a new set of page flags for a page table, with default values.
    #[must_use]
    pub const fn new_table() -> Self {
        Self(Arch::PAGE_FLAG_TABLE_DEFAULTS)
    }

    /// Creates a new set of page flags for a text segment, which is executable, and writable in debug builds.
    #[must_use]
    pub const fn new_for_text_segment() -> Self {
        if cfg!(debug_assertions) {
            Self::new().executable().writable() // for inserting breakpoints
        } else {
            Self::new().executable()
        }
    }

    /// Creates a new set of page flags for a read-only data segment.
    #[must_use]
    pub fn new_for_rodata_segment() -> Self {
        Self::new()
    }

    /// Creates a new set of page flags for a writable data segment.
    #[must_use]
    pub fn new_for_data_segment() -> Self {
        Self::new().writable()
    }

    /// Creates a new set of page flags for a device memory mapping.
    #[cfg(target_arch = "aarch64")]
    #[must_use]
    pub fn new_device() -> Self {
        Self::from_raw(Arch::PAGE_FLAG_DEVICE)
    }

    /// Creates a new set of page flags from a raw unsigned double word value.
    #[must_use]
    pub const fn from_raw(raw: usize) -> Self {
        Self(raw)
    }

    /// Returns the raw value of the page flags as an unsigned double word value.
    #[must_use]
    pub const fn raw(&self) -> usize {
        self.0
    }

    /// Returns `true` if the page flags contain the given flags.
    /// Always returns `false` for empty flags.
    #[must_use]
    pub const fn has_flags(&self, flag: usize) -> bool {
        self.0 & flag == flag && flag != 0
    }

    /// Sets or clears the given flag in the page flags.
    #[must_use]
    pub const fn with_flag(&self, flag: usize, value: bool) -> Self {
        if value {
            Self(self.0 | flag)
        } else {
            Self(self.0 & !flag)
        }
    }

    /// Returns `true` if the page flags contain the "present" flag.
    #[must_use]
    pub const fn is_present(&self) -> bool {
        self.has_flags(Arch::PAGE_FLAG_PRESENT)
    }

    /// Sets the "present" flag in the page flags.
    #[must_use]
    pub const fn present(self) -> Self {
        self.with_flag(Arch::PAGE_FLAG_PRESENT, true)
    }

    /// Returns `true` if the page flags contain the "executable" flag.
    #[must_use]
    pub const fn is_executable(&self) -> bool {
        self.0 & (Arch::PAGE_FLAG_EXECUTABLE | Arch::PAGE_FLAG_NON_EXECUTABLE)
            == Arch::PAGE_FLAG_EXECUTABLE
    }

    /// Sets the "executable" flag in the page flags, clearing the "non-executable" flag.
    #[must_use]
    pub const fn executable(self) -> Self {
        self.with_flag(Arch::PAGE_FLAG_EXECUTABLE, true)
            .with_flag(Arch::PAGE_FLAG_NON_EXECUTABLE, false)
    }

    /// Returns `true` if the page flags contain the "writable" flag.
    #[must_use]
    pub const fn is_writable(&self) -> bool {
        self.0 & (Arch::PAGE_FLAG_READONLY | Arch::PAGE_FLAG_READWRITE) == Arch::PAGE_FLAG_READWRITE
    }

    /// Sets the "writable" flag in the page flags, clearing the "readonly" flag.
    #[must_use]
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
