use core::fmt::{self, Debug, Display};

use derive_more::*;

use crate::arch::{Arch, ArchTrait};

use super::{MemError, hhdm_physical_offset};

#[inline]
pub const fn canonicalize_physaddr(addr: usize) -> usize {
    addr & 0x000F_FFFF_FFFF_FFFF
}

#[inline]
pub const fn canonicalize_virtaddr(addr: usize) -> usize {
    ((addr << 16) as i64 >> 16) as usize
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(transparent)]
pub struct PhysAddr(usize);

impl Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhysAddr({:#x})", self.0)
    }
}

impl Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
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

    pub const fn add(self, offset: usize) -> Self {
        Self::new_canonical(self.value() + offset)
    }

    pub fn as_hhdm_virt(self) -> VirtAddr {
        VirtAddr::new_canonical(self.value() + hhdm_physical_offset())
    }

    pub fn frame_index(self) -> FrameCount {
        FrameCount::from_bytes(self.value())
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, Add, Sub, Mul, Div, Rem, Deref)]
#[repr(transparent)]
pub struct VirtAddr(usize);

impl Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VirtAddr({:#x})", self.0)
    }
}

impl Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

impl VirtAddr {
    pub const NULL: Self = unsafe { Self::new_unchecked(0) };
    pub const MIN_HIGH: Self = unsafe { Self::new_unchecked(0xFFFF_8000_0000_0000) };

    pub const fn new_canonical(addr: usize) -> Self {
        unsafe { Self::new_unchecked(canonicalize_virtaddr(addr)) }
    }

    pub const fn new(addr: usize) -> Result<Self, MemError> {
        if canonicalize_virtaddr(addr) == addr {
            Ok(unsafe { Self::new_unchecked(addr) })
        } else {
            Err(MemError::NonCanonicalVirtAddr(addr))
        }
    }

    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }

    pub fn from_ref<T: 'static>(val: &T) -> Self {
        Self::new_canonical(val as *const _ as usize)
    }

    pub fn from_mut<T: 'static>(val: &mut T) -> Self {
        Self::new_canonical(val as *mut _ as usize)
    }

    pub const fn value(self) -> usize {
        self.0
    }

    pub const fn is_null(self) -> bool {
        self.value() == Self::NULL.value()
    }

    pub const fn is_canonical(self) -> bool {
        self.0 == canonicalize_virtaddr(self.0)
    }

    pub const fn as_raw_ptr<T: 'static>(self) -> *const T {
        self.value() as *const T
    }

    pub const fn as_raw_ptr_mut<T: 'static>(self) -> *mut T {
        self.value() as *mut T
    }

    pub const fn is_aligned(self, align: usize) -> bool {
        self.value() % align == 0
    }

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

    pub fn as_hhdm_phys(self) -> PhysAddr {
        PhysAddr::new_canonical(self.value() - hhdm_physical_offset())
    }

    pub const fn add(self, offset: usize) -> Self {
        Self::new_canonical(self.value() + offset)
    }

    pub const fn offset(self, offset: isize) -> Self {
        Self::new_canonical((self.value() as isize + offset) as usize)
    }

    pub unsafe fn read<T: Copy + 'static>(self) -> Result<T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { self.as_raw_ptr::<T>().read() })
    }

    pub unsafe fn read_volatile<T: Copy + 'static>(self) -> Result<T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { self.as_raw_ptr::<T>().read_volatile() })
    }

    pub unsafe fn read_bytes(self, buf: &mut [u8]) -> Result<usize, MemError> {
        self.align_ok::<u8>()?; // check for null and canonicalness
        unsafe {
            core::ptr::copy(self.as_raw_ptr(), buf.as_mut_ptr(), buf.len());
        }
        Ok(buf.len())
    }

    pub unsafe fn write<T: Copy + 'static>(self, val: T) -> Result<(), MemError> {
        self.align_ok::<T>()?;
        unsafe {
            self.as_raw_ptr_mut::<T>().write(val);
        }
        Ok(())
    }

    pub unsafe fn write_volatile<T: Copy + 'static>(self, val: T) -> Result<(), MemError> {
        self.align_ok::<T>()?;
        unsafe {
            self.as_raw_ptr_mut::<T>().write_volatile(val);
        }
        Ok(())
    }

    pub unsafe fn write_bytes(self, buf: &[u8]) -> Result<usize, MemError> {
        self.align_ok::<u8>()?; // check for null and canonicalness
        self.offset(buf.len() as isize).align_ok::<u8>()?;
        unsafe {
            core::slice::from_raw_parts_mut(self.as_raw_ptr_mut(), buf.len()).copy_from_slice(buf);
        }
        Ok(buf.len())
    }

    pub unsafe fn fill(self, val: u8, len: usize) -> Result<usize, MemError> {
        self.align_ok::<u8>()?;
        self.offset(len as isize).align_ok::<u8>()?;
        unsafe {
            self.as_raw_ptr_mut::<u8>().write_bytes(val, len);
        }
        Ok(len)
    }

    pub unsafe fn deref<'a, T: 'static>(self) -> Result<&'a T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { &*self.as_raw_ptr() })
    }

    pub unsafe fn deref_mut<'a, T: 'static>(self) -> Result<&'a mut T, MemError> {
        self.align_ok::<T>()?;
        Ok(unsafe { &mut *self.as_raw_ptr_mut() })
    }

    pub const fn align_down(self, align: usize) -> Self {
        VirtAddr::new_canonical(self.value() / align * align)
    }

    pub const fn align_up(self, align: usize) -> Self {
        VirtAddr::new_canonical(self.value().div_ceil(align) * align)
    }

    pub const fn page_table_index(self, level: usize) -> usize {
        ((self.value() / Arch::PAGE_SIZE) >> (Arch::PAGE_ENTRY_SHIFT * level))
            & Arch::PAGE_ENTRY_MASK
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
