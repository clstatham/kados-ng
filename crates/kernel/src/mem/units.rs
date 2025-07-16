use core::fmt::{self, Debug, Display};

use derive_more::*;

use crate::{
    HHDM_PHYSICAL_OFFSET,
    arch::{Arch, Architecture},
};

use super::{MemError, paging::table::PageTableLevel};

/// Canonicalizes a physical address by masking the upper bits.
#[inline]
pub const fn canonicalize_physaddr(addr: usize) -> usize {
    addr & 0x000F_FFFF_FFFF_FFFF
}

/// Canonicalizes a virtual address by shifting it to ensure it fits within the canonical range.
#[inline]
pub const fn canonicalize_virtaddr(addr: usize) -> usize {
    ((addr << 16) as i64 >> 16) as usize
}

/// Represents an address in physical memory.
#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, Default)]
#[repr(transparent)]
pub struct PhysAddr(usize);

impl Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhysAddr({:#016x})", self.0)
    }
}

impl Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#016x}", self.0)
    }
}

impl PhysAddr {
    /// A physical address that is guaranteed to be null (0).
    pub const NULL: Self = unsafe { Self::new_unchecked(0) };

    /// Creates a new physical address that is guaranteed to be canonical (canonicalized with [`canonicalize_physaddr`] if it isn't).
    pub const fn new_canonical(addr: usize) -> Self {
        unsafe { Self::new_unchecked(canonicalize_physaddr(addr)) }
    }

    /// Creates a new physical address, checking if it is canonical.
    pub const fn new(addr: usize) -> Result<Self, MemError> {
        if canonicalize_physaddr(addr) == addr {
            Ok(unsafe { Self::new_unchecked(addr) })
        } else {
            Err(MemError::NonCanonicalPhysAddr(addr))
        }
    }

    /// Creates a new physical address without checking if it is canonical.
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }

    /// Returns the raw address value as an unsigned integer.
    pub const fn value(self) -> usize {
        self.0
    }

    /// Returns `true` if the address is null (0).
    pub const fn is_null(self) -> bool {
        self.value() == Self::NULL.value()
    }

    /// Returns `true` if the address is canonical.
    pub const fn is_canonical(self) -> bool {
        self.0 == canonicalize_physaddr(self.0)
    }

    /// Returns `true` if the address is aligned to the specified alignment.
    pub const fn is_aligned(self, align: usize) -> bool {
        self.value().is_multiple_of(align) || self.value() & (align - 1) == 0
    }

    /// Returns the sum of the address and an offset, ensuring the result is canonical.
    pub const fn add_bytes(self, offset: usize) -> Self {
        Self::new_canonical(self.value() + offset)
    }

    /// Returns the address as a [`VirtAddr`] in the HHDM (High Half Direct Mapped) space.
    pub const fn as_hhdm_virt(self) -> VirtAddr {
        VirtAddr::new_canonical(self.value() + HHDM_PHYSICAL_OFFSET)
    }

    /// Returns the address as a [`VirtAddr`] in the identity-mapped virtual address space.
    pub const fn as_identity_virt(self) -> VirtAddr {
        VirtAddr::new_canonical(self.value())
    }

    /// Returns the index of the frame corresponding to this physical address.
    pub const fn frame_index(self) -> FrameCount {
        FrameCount::from_bytes(self.value())
    }

    /// Returns the address aligned down to the nearest multiple of the specified alignment.
    pub const fn align_down(self, align: usize) -> Self {
        PhysAddr::new_canonical(self.value() / align * align)
    }

    /// Returns the address aligned up to the nearest multiple of the specified alignment.
    pub const fn align_up(self, align: usize) -> Self {
        PhysAddr::new_canonical(self.value().div_ceil(align) * align)
    }
}

/// Represents an address in virtual memory.
#[derive(
    Clone,
    Copy,
    PartialEq,
    PartialOrd,
    Eq,
    Ord,
    Hash,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Deref,
    Default,
    UpperHex,
    LowerHex,
    Binary,
)]
#[repr(transparent)]
pub struct VirtAddr(usize);

impl Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VirtAddr({:#016x})", self.0)
    }
}

impl Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#016x}", self.0)
    }
}

impl VirtAddr {
    /// The maximum low virtual address, which is the highest address in the low memory region.
    pub const MAX_LOW: Self = unsafe { Self::new_unchecked(0x0000_7000_0000_0000) };
    /// The minimum high virtual address, which is the lowest address in the high memory region.
    pub const MIN_HIGH: Self = unsafe { Self::new_unchecked(0xffff_8000_0000_0000) };
    /// A virtual address that is guaranteed to be null (0).
    pub const NULL: Self = unsafe { Self::new_unchecked(0) };

    /// Creates a new virtual address that is guaranteed to be canonical (canonicalized with [`canonicalize_virtaddr`] if it isn't).
    #[inline(always)]
    pub const fn new_canonical(addr: usize) -> Self {
        unsafe { Self::new_unchecked(canonicalize_virtaddr(addr)) }
    }

    /// Creates a new virtual address, checking if it is canonical.
    #[inline(always)]
    pub const fn new(addr: usize) -> Result<Self, MemError> {
        if canonicalize_virtaddr(addr) == addr {
            Ok(unsafe { Self::new_unchecked(addr) })
        } else {
            Err(MemError::NonCanonicalVirtAddr(addr))
        }
    }

    /// Creates a new virtual address without checking if it is canonical.
    #[inline(always)]
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }

    /// Creates a new virtual address from a reference, ensuring it is canonical.
    #[inline(always)]
    pub fn from_ref<T: 'static>(val: &T) -> Self {
        Self::new_canonical(val as *const _ as usize)
    }

    /// Creates a new virtual address from a mutable reference, ensuring it is canonical.
    #[inline(always)]
    pub fn from_mut<T: 'static>(val: &mut T) -> Self {
        Self::new_canonical(val as *mut _ as usize)
    }

    /// Returns the raw address value as an unsigned integer.
    #[inline(always)]
    pub const fn value(self) -> usize {
        self.0
    }

    /// Returns `true` if the address is null (0).
    #[inline(always)]
    pub const fn is_null(self) -> bool {
        self.value() == Self::NULL.value()
    }

    /// Returns `true` if the address is canonical.
    #[inline(always)]
    pub const fn is_canonical(self) -> bool {
        self.0 == canonicalize_virtaddr(self.0)
    }

    /// Casts the address to a raw pointer of type `*const T`.
    #[inline(always)]
    pub const fn as_raw_ptr<T: 'static>(self) -> *const T {
        self.value() as *const T
    }

    /// Casts the address to a raw mutable pointer of type `*mut T`.
    #[inline(always)]
    pub const fn as_raw_ptr_mut<T: 'static>(self) -> *mut T {
        self.value() as *mut T
    }

    /// Returns `true` if the address is aligned to the specified alignment.
    #[inline(always)]
    pub const fn is_aligned(self, align: usize) -> bool {
        self.value().is_multiple_of(align) || self.value() & (align - 1) == 0
    }

    /// Checks if the address is aligned to the alignment of type `T`, returning an error if it is not.
    /// Also checks if the address is null or non-canonical.
    ///
    /// Essentially, this method ensures that the address is suitable for accessing data of type `T`.
    #[inline(always)]
    pub const fn align_ok<T: Sized>(self) -> Result<(), MemError> {
        if self.is_null() {
            return Err(MemError::NullVirtAddr);
        }
        if !self.is_aligned(align_of::<T>()) {
            return Err(MemError::UnalignedVirtAddr(self, align_of::<T>()));
        }
        if !self.is_canonical() {
            return Err(MemError::NonCanonicalVirtAddr(self.value()));
        }
        Ok(())
    }

    /// Returns the address as a [`PhysAddr`] in the HHDM (High Half Direct Mapped) space.
    #[inline(always)]
    pub fn as_hhdm_phys(self) -> PhysAddr {
        PhysAddr::new_canonical(self.value() - HHDM_PHYSICAL_OFFSET)
    }

    /// Returns the sum of the address and an offset, ensuring the result is canonical.
    #[inline(always)]
    pub const fn add_bytes(self, offset: usize) -> Self {
        Self::new_canonical(self.value() + offset)
    }

    /// Returns the signed sum of the address and an offset, ensuring the result is canonical.
    #[inline(always)]
    pub const fn offset_bytes(self, offset: isize) -> Self {
        Self::new_canonical((self.value() as isize + offset) as usize)
    }

    /// Reads a value of type `T` from the address, ensuring it is aligned, canonical, and non-null.
    #[inline(always)]
    pub unsafe fn read<T: Copy + 'static>(self) -> Result<T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { self.as_raw_ptr::<T>().read() })
    }

    /// Reads a volatile value of type `T` from the address, ensuring it is aligned, canonical, and non-null.
    #[inline(always)]
    pub unsafe fn read_volatile<T: Copy + 'static>(self) -> Result<T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { self.as_raw_ptr::<T>().read_volatile() })
    }

    /// Reads a slice of bytes from the address, ensuring it is aligned, canonical, and non-null.
    #[inline(always)]
    pub unsafe fn read_bytes(self, buf: &mut [u8]) -> Result<usize, MemError> {
        self.align_ok::<u8>()?; // check for null and canonicalness
        if buf.is_empty() {
            return Ok(0);
        }
        self.add_bytes(buf.len()).align_ok::<u8>()?; // check for out-of-bounds access
        unsafe {
            core::ptr::copy(self.as_raw_ptr(), buf.as_mut_ptr(), buf.len());
        }
        Ok(buf.len())
    }

    /// Writes a value of type `T` to the address, ensuring it is aligned, canonical, and non-null.
    #[inline(always)]
    pub unsafe fn write<T: 'static>(self, val: T) -> Result<(), MemError> {
        self.align_ok::<T>()?;
        unsafe {
            self.as_raw_ptr_mut::<T>().write(val);
        }
        Ok(())
    }

    /// Writes a volatile value of type `T` to the address, ensuring it is aligned, canonical, and non-null.
    #[inline(always)]
    pub unsafe fn write_volatile<T: 'static>(self, val: T) -> Result<(), MemError> {
        self.align_ok::<T>()?;
        unsafe {
            self.as_raw_ptr_mut::<T>().write_volatile(val);
        }
        Ok(())
    }

    /// Writes a slice of bytes to the address, ensuring it is aligned, canonical, and non-null.
    #[inline(always)]
    pub unsafe fn write_bytes(self, buf: &[u8]) -> Result<usize, MemError> {
        self.align_ok::<u8>()?; // check for null and canonicalness
        self.add_bytes(buf.len()).align_ok::<u8>()?; // check for out-of-bounds access
        unsafe {
            core::slice::from_raw_parts_mut(self.as_raw_ptr_mut(), buf.len()).copy_from_slice(buf);
        }
        Ok(buf.len())
    }

    /// Fills a length of bytes at the address with a specified value, ensuring it is aligned, canonical, and non-null.
    #[inline(always)]
    pub unsafe fn fill(self, val: u8, len: usize) -> Result<usize, MemError> {
        self.align_ok::<u8>()?;
        self.add_bytes(len).align_ok::<u8>()?; // check for out-of-bounds access
        unsafe {
            self.as_raw_ptr_mut::<u8>().write_bytes(val, len);
        }
        Ok(len)
    }

    /// Returns a reference to the value at the address, ensuring it is aligned, canonical, and non-null.
    #[inline(always)]
    pub unsafe fn deref<'a, T: 'static>(self) -> Result<&'a T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { &*self.as_raw_ptr() })
    }

    /// Returns a mutable reference to the value at the address, ensuring it is aligned, canonical, and non-null.
    #[inline(always)]
    pub unsafe fn deref_mut<'a, T: 'static>(self) -> Result<&'a mut T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { &mut *self.as_raw_ptr_mut() })
    }

    // Returns the address aligned down to the nearest multiple of the specified alignment.
    #[inline(always)]
    pub const fn align_down(self, align: usize) -> Self {
        VirtAddr::new_canonical(self.value() / align * align)
    }

    /// Returns the address aligned up to the nearest multiple of the specified alignment.
    #[inline(always)]
    pub const fn align_up(self, align: usize) -> Self {
        VirtAddr::new_canonical(self.value().div_ceil(align) * align)
    }

    /// Returns the index of the page table entry corresponding to this virtual address at the specified page table level.
    #[inline(always)]
    pub const fn page_table_index(self, level: PageTableLevel) -> usize {
        (self.value() >> level.shift()) & Arch::PAGE_ENTRY_MASK
    }
}

/// Represents a frame in physical memory.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Frame {
    index: usize,
}

/// Represents a group of contiguous frames in memory.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub struct FrameCount(usize);

impl FrameCount {
    /// A frame count that is guaranteed to be empty (0).
    pub const EMPTY: Self = Self(0);
    /// A frame count that is guaranteed to contain one frame (1).
    pub const ONE: Self = Self(1);

    /// Creates a new frame count from the number of frames.
    pub const fn new(count: usize) -> Self {
        Self(count)
    }

    /// Creates a new frame count from the number of bytes, ensuring it is rounded up to the nearest frame size.
    pub const fn from_bytes(bytes: usize) -> Self {
        Self(bytes.div_ceil(Arch::PAGE_SIZE))
    }

    /// Returns the number of frames in this frame count.
    pub const fn frame_count(self) -> usize {
        self.0
    }

    /// Returns the index of the frame corresponding to this frame count, starting from 0.
    pub const fn frame_index(self) -> usize {
        self.0
    }

    /// Returns the number of bytes represented by this frame count.
    pub const fn to_bytes(self) -> usize {
        self.0 * Arch::PAGE_SIZE
    }
}
