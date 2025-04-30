use core::fmt::{self, Debug, Display};

use derive_more::*;

use super::MmuError;

#[inline]
pub const fn canonicalize_physaddr(addr: u64) -> u64 {
    addr & 0x000F_FFFF_FFFF_FFFF
}

#[inline]
pub const fn canonicalize_virtaddr(addr: u64) -> u64 {
    ((addr << 16) as i64 >> 16) as u64
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, Add, Sub, Mul, Div, Rem, Deref)]
#[repr(transparent)]
pub struct PhysAddr(u64);

impl Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PhysAddr").field(self).finish()
    }
}

impl Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

impl PhysAddr {
    pub const NULL: Self = unsafe { Self::new_unchecked(0) };

    pub const fn new_canonical(addr: u64) -> Self {
        unsafe { Self::new_unchecked(canonicalize_physaddr(addr)) }
    }

    pub const fn new(addr: u64) -> Result<Self, MmuError> {
        if canonicalize_physaddr(addr) == addr {
            Ok(unsafe { Self::new_unchecked(addr) })
        } else {
            Err(MmuError::NonCanonicalPhysAddr(addr))
        }
    }

    pub const unsafe fn new_unchecked(addr: u64) -> Self {
        debug_assert!(
            canonicalize_physaddr(addr) == addr,
            "PhysAddr::new_unchecked() called on non-canonical physical address"
        );
        Self(addr)
    }

    pub fn kernel_base() -> Self {
        Self::new_canonical(super::hhdm_physical_offset())
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    pub const fn is_null(self) -> bool {
        self.value() == Self::NULL.value()
    }

    pub const fn is_canonical(self) -> bool {
        self.0 == canonicalize_physaddr(self.0)
    }

    pub fn as_hhdm_virt(self) -> VirtAddr {
        VirtAddr::new_canonical(self.value()) + VirtAddr::kernel_base()
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash, Add, Sub, Mul, Div, Rem, Deref)]
#[repr(transparent)]
pub struct VirtAddr(u64);

impl Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("VirtAddr").field(self).finish()
    }
}

impl Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

impl VirtAddr {
    pub const NULL: Self = unsafe { Self::new_unchecked(0) };

    pub const fn new_canonical(addr: u64) -> Self {
        unsafe { Self::new_unchecked(canonicalize_virtaddr(addr)) }
    }

    pub const fn new(addr: u64) -> Result<Self, MmuError> {
        if canonicalize_virtaddr(addr) == addr {
            Ok(unsafe { Self::new_unchecked(addr) })
        } else {
            Err(MmuError::NonCanonicalVirtAddr(addr))
        }
    }

    pub const unsafe fn new_unchecked(addr: u64) -> Self {
        debug_assert!(
            canonicalize_virtaddr(addr) == addr,
            "VirtAddr::new_unchecked() called on non-canonical virtual address"
        );
        Self(addr)
    }

    pub fn kernel_base() -> Self {
        Self::new_canonical(super::hhdm_physical_offset())
    }

    pub fn from_ref<T: 'static>(val: &T) -> Self {
        Self::new_canonical(val as *const _ as u64)
    }

    pub fn from_mut<T: 'static>(val: &mut T) -> Self {
        Self::new_canonical(val as *mut _ as u64)
    }

    pub const fn value(self) -> u64 {
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

    pub fn as_hhdm_phys(self) -> PhysAddr {
        PhysAddr::new_canonical(self.value()) - PhysAddr::kernel_base()
    }

    pub const fn is_aligned(self, align: u64) -> bool {
        self.value() % align == 0
    }

    pub const fn align_ok<T: Sized>(self) -> Result<(), MmuError> {
        if self.is_null() {
            return Err(MmuError::NullVirtAddr);
        }
        if !self.is_aligned(align_of::<T>() as u64) {
            return Err(MmuError::UnalignedVirtAddr(self, align_of::<T>() as u64));
        }
        if !self.is_canonical() {
            return Err(MmuError::NonCanonicalVirtAddr(self.value()));
        }
        Ok(())
    }

    pub const fn offset(self, offset: isize) -> Self {
        Self::new_canonical((self.value() as isize + offset) as u64)
    }

    pub unsafe fn read<T: Copy + 'static>(self) -> Result<T, MmuError> {
        self.align_ok::<T>()?;
        Ok(unsafe { self.as_raw_ptr::<T>().read() })
    }

    pub unsafe fn read_volatile<T: Copy + 'static>(self) -> Result<T, MmuError> {
        self.align_ok::<T>()?;
        Ok(unsafe { self.as_raw_ptr::<T>().read_volatile() })
    }

    pub unsafe fn read_bytes(self, buf: &mut [u8]) -> Result<usize, MmuError> {
        self.align_ok::<u8>()?; // check for null and canonicalness
        unsafe {
            core::ptr::copy(self.as_raw_ptr(), buf.as_mut_ptr(), buf.len());
        }
        Ok(buf.len())
    }

    pub unsafe fn write<T: Copy + 'static>(self, val: T) -> Result<(), MmuError> {
        self.align_ok::<T>()?;
        unsafe {
            self.as_raw_ptr_mut::<T>().write(val);
        }
        Ok(())
    }

    pub unsafe fn write_volatile<T: Copy + 'static>(self, val: T) -> Result<(), MmuError> {
        self.align_ok::<T>()?;
        unsafe {
            self.as_raw_ptr_mut::<T>().write_volatile(val);
        }
        Ok(())
    }

    pub unsafe fn write_bytes(self, buf: &[u8]) -> Result<usize, MmuError> {
        self.align_ok::<u8>()?; // check for null and canonicalness
        self.offset(buf.len() as isize).align_ok::<u8>()?;
        unsafe {
            core::slice::from_raw_parts_mut(self.as_raw_ptr_mut(), buf.len()).copy_from_slice(buf);
        }
        Ok(buf.len())
    }

    pub unsafe fn fill(self, val: u8, len: usize) -> Result<usize, MmuError> {
        self.align_ok::<u8>()?;
        self.offset(len as isize).align_ok::<u8>()?;
        unsafe {
            self.as_raw_ptr_mut::<u8>().write_bytes(val, len);
        }
        Ok(len)
    }

    pub unsafe fn deref<'a, T: 'static>(self) -> Result<&'a T, MmuError> {
        self.align_ok::<T>()?;
        Ok(unsafe { &*self.as_raw_ptr() })
    }

    pub unsafe fn deref_mut<'a, T: 'static>(self) -> Result<&'a mut T, MmuError> {
        self.align_ok::<T>()?;
        Ok(unsafe { &mut *self.as_raw_ptr_mut() })
    }

    pub const fn align_down(self, align: u64) -> Self {
        VirtAddr::new_canonical(self.value() / align * align)
    }

    pub const fn align_up(self, align: u64) -> Self {
        VirtAddr::new_canonical(self.value().div_ceil(align) * align)
    }
}
