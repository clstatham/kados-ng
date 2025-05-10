use core::fmt::{self, Debug, Display};

use derive_more::*;

use crate::{
    HHDM_PHYSICAL_OFFSET,
    arch::{Arch, ArchTrait},
};

use super::{MemError, paging::table::PageTableLevel};

#[inline]
pub const fn canonicalize_physaddr(addr: usize) -> usize {
    addr & 0x000F_FFFF_FFFF_FFFF
}

#[inline]
pub const fn canonicalize_virtaddr(addr: usize) -> usize {
    ((addr << 16) as i64 >> 16) as usize
}

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
    pub const NULL: Self = unsafe { Self::new_unchecked(0) };

    pub const fn new_canonical(addr: usize) -> Self {
        unsafe { Self::new_unchecked(canonicalize_physaddr(addr)) }
    }

    pub const fn new(addr: usize) -> Result<Self, MemError> {
        if canonicalize_physaddr(addr) == addr {
            Ok(unsafe { Self::new_unchecked(addr) })
        } else {
            Err(MemError::NonCanonicalPhysAddr(addr))
        }
    }

    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        debug_assert!(
            canonicalize_physaddr(addr) == addr,
            "PhysAddr::new_unchecked() called on non-canonical physical address"
        );
        Self(addr)
    }

    pub const fn value(self) -> usize {
        self.0
    }

    pub const fn is_null(self) -> bool {
        self.value() == Self::NULL.value()
    }

    pub const fn is_canonical(self) -> bool {
        self.0 == canonicalize_physaddr(self.0)
    }

    pub const fn is_aligned(self, align: usize) -> bool {
        self.0 % align == 0
    }

    pub const fn add_bytes(self, offset: usize) -> Self {
        Self::new_canonical(self.value() + offset)
    }

    pub const fn as_hhdm_virt(self) -> VirtAddr {
        VirtAddr::new_canonical(self.value() + HHDM_PHYSICAL_OFFSET)
    }

    pub const fn as_identity_virt(self) -> VirtAddr {
        VirtAddr::new_canonical(self.value())
    }

    pub const fn frame_index(self) -> FrameCount {
        FrameCount::from_bytes(self.value())
    }

    pub const fn align_down(self, align: usize) -> Self {
        PhysAddr::new_canonical(self.value() / align * align)
    }

    pub const fn align_up(self, align: usize) -> Self {
        PhysAddr::new_canonical(self.value().div_ceil(align) * align)
    }
}

#[derive(
    Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, Add, Sub, Mul, Div, Rem, Deref, Default,
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
    pub const MAX_LOW: Self = unsafe { Self::new_unchecked(0x0000_7000_0000_0000) };
    pub const MIN_HIGH: Self = unsafe { Self::new_unchecked(0xffff_8000_0000_0000) };
    pub const NULL: Self = unsafe { Self::new_unchecked(0) };

    #[inline(always)]
    pub const fn new_canonical(addr: usize) -> Self {
        unsafe { Self::new_unchecked(canonicalize_virtaddr(addr)) }
    }

    #[inline(always)]
    pub const fn new(addr: usize) -> Result<Self, MemError> {
        if canonicalize_virtaddr(addr) == addr {
            Ok(unsafe { Self::new_unchecked(addr) })
        } else {
            Err(MemError::NonCanonicalVirtAddr(addr))
        }
    }

    #[inline(always)]
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }

    #[inline(always)]
    pub fn from_ref<T: 'static>(val: &T) -> Self {
        Self::new_canonical(val as *const _ as usize)
    }

    #[inline(always)]
    pub fn from_mut<T: 'static>(val: &mut T) -> Self {
        Self::new_canonical(val as *mut _ as usize)
    }

    #[inline(always)]
    pub const fn value(self) -> usize {
        self.0
    }

    #[inline(always)]
    pub const fn is_null(self) -> bool {
        self.value() == Self::NULL.value()
    }

    #[inline(always)]
    pub const fn is_canonical(self) -> bool {
        self.0 == canonicalize_virtaddr(self.0)
    }

    #[inline(always)]
    pub const fn as_raw_ptr<T: 'static>(self) -> *const T {
        self.value() as *const T
    }

    #[inline(always)]
    pub const fn as_raw_ptr_mut<T: 'static>(self) -> *mut T {
        self.value() as *mut T
    }

    #[inline(always)]
    pub const fn is_aligned(self, align: usize) -> bool {
        self.value() % align == 0
    }

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

    #[inline(always)]
    pub fn as_hhdm_phys(self) -> PhysAddr {
        PhysAddr::new_canonical(self.value() - HHDM_PHYSICAL_OFFSET)
    }

    #[inline(always)]
    pub const fn add_bytes(self, offset: usize) -> Self {
        Self::new_canonical(self.value() + offset)
    }

    #[inline(always)]
    pub const fn offset_bytes(self, offset: isize) -> Self {
        Self::new_canonical((self.value() as isize + offset) as usize)
    }

    #[inline(always)]
    pub unsafe fn read<T: Copy + 'static>(self) -> Result<T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { self.as_raw_ptr::<T>().read() })
    }

    #[inline(always)]
    pub unsafe fn read_volatile<T: Copy + 'static>(self) -> Result<T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { self.as_raw_ptr::<T>().read_volatile() })
    }

    #[inline(always)]
    pub unsafe fn read_bytes(self, buf: &mut [u8]) -> Result<usize, MemError> {
        self.align_ok::<u8>()?; // check for null and canonicalness
        unsafe {
            core::ptr::copy(self.as_raw_ptr(), buf.as_mut_ptr(), buf.len());
        }
        Ok(buf.len())
    }

    #[inline(always)]
    pub unsafe fn write<T: 'static>(self, val: T) -> Result<(), MemError> {
        self.align_ok::<T>()?;
        unsafe {
            self.as_raw_ptr_mut::<T>().write(val);
        }
        Ok(())
    }

    #[inline(always)]
    pub unsafe fn write_volatile<T: 'static>(self, val: T) -> Result<(), MemError> {
        self.align_ok::<T>()?;
        unsafe {
            self.as_raw_ptr_mut::<T>().write_volatile(val);
        }
        Ok(())
    }

    #[inline(always)]
    pub unsafe fn write_bytes(self, buf: &[u8]) -> Result<usize, MemError> {
        self.align_ok::<u8>()?; // check for null and canonicalness
        self.offset_bytes(buf.len() as isize).align_ok::<u8>()?;
        unsafe {
            core::slice::from_raw_parts_mut(self.as_raw_ptr_mut(), buf.len()).copy_from_slice(buf);
        }
        Ok(buf.len())
    }

    #[inline(always)]
    pub unsafe fn fill(self, val: u8, len: usize) -> Result<usize, MemError> {
        self.align_ok::<u8>()?;
        self.offset_bytes(len as isize).align_ok::<u8>()?;
        unsafe {
            self.as_raw_ptr_mut::<u8>().write_bytes(val, len);
        }
        Ok(len)
    }

    #[inline(always)]
    pub unsafe fn deref<'a, T: 'static>(self) -> Result<&'a T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { &*self.as_raw_ptr() })
    }

    #[inline(always)]
    pub unsafe fn deref_mut<'a, T: 'static>(self) -> Result<&'a mut T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { &mut *self.as_raw_ptr_mut() })
    }

    #[inline(always)]
    pub const fn align_down(self, align: usize) -> Self {
        VirtAddr::new_canonical(self.value() / align * align)
    }

    #[inline(always)]
    pub const fn align_up(self, align: usize) -> Self {
        VirtAddr::new_canonical(self.value().div_ceil(align) * align)
    }

    #[inline(always)]
    pub const fn page_table_index(self, level: PageTableLevel) -> usize {
        (self.value() >> level.shift()) & Arch::PAGE_ENTRY_MASK
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Frame {
    index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub struct FrameCount(usize);

impl FrameCount {
    pub const EMPTY: Self = Self(0);
    pub const ONE: Self = Self(1);

    pub const fn new(count: usize) -> Self {
        Self(count)
    }

    pub const fn from_bytes(bytes: usize) -> Self {
        Self(bytes / Arch::PAGE_SIZE)
    }

    pub const fn frame_count(self) -> usize {
        self.0
    }

    pub const fn frame_index(self) -> usize {
        self.0
    }

    pub const fn to_bytes(self) -> usize {
        self.0 * Arch::PAGE_SIZE
    }
}
