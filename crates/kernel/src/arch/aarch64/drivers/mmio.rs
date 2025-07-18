use core::{
    arch::asm,
    fmt::{Binary, Debug, LowerHex, UpperHex},
    marker::PhantomData,
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not},
};

use crate::mem::units::VirtAddr;

pub trait MmioValue:
    'static
    + Copy
    + Debug
    + Binary
    + LowerHex
    + UpperHex
    + PartialEq
    + Eq
    + PartialOrd
    + Ord
    + BitAndAssign
    + BitOrAssign
    + Not<Output = Self>
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
{
    const ZERO: Self;
}

impl MmioValue for u8 {
    const ZERO: Self = 0;
}

impl MmioValue for u16 {
    const ZERO: Self = 0;
}

impl MmioValue for u32 {
    const ZERO: Self = 0;
}

impl MmioValue for u64 {
    const ZERO: Self = 0;
}

#[derive(Debug, Default)]
pub struct Mmio<T: MmioValue> {
    pub addr: VirtAddr,
    _marker: PhantomData<fn() -> T>,
}

impl<T: MmioValue> Mmio<T> {
    #[must_use]
    pub const fn new(addr: VirtAddr) -> Self {
        Self {
            addr,
            _marker: PhantomData,
        }
    }

    /// Reads a value from the MMIO address at the specified offset.
    ///
    /// # Panics
    ///
    /// This function will panic if the read operation fails.
    #[inline]
    #[must_use]
    pub unsafe fn read(&self, offset: usize) -> T {
        unsafe {
            asm!("dsb sy", "isb");
            self.addr.add_bytes(offset).read_volatile().unwrap()
        }
    }

    /// Writes a value to the MMIO address at the specified offset.
    ///
    /// # Panics
    ///
    /// This function will panic if the write operation fails.
    #[inline]
    pub unsafe fn write(&mut self, offset: usize, value: T) {
        unsafe {
            self.addr.add_bytes(offset).write_volatile(value).unwrap();
            asm!("dsb sy", "isb");
        }
    }

    /// Writes a value to the MMIO address at the specified offset and asserts that the value was written correctly.
    ///
    /// # Panics
    ///
    /// This function will panic if the read value does not match the written value.
    #[inline]
    #[track_caller]
    pub unsafe fn write_assert(&mut self, offset: usize, value: T) {
        unsafe {
            self.write(offset, value);
            assert_eq!(self.read(offset), value);
        }
    }

    /// Reads a value from the MMIO address at the specified offset and sets some of its bits.
    ///
    /// # Panics
    ///
    /// This function will panic if either the read or write operation fails.
    #[inline]
    pub unsafe fn set(&mut self, offset: usize, bits: T) {
        unsafe {
            let mut value = self.read(offset);
            value |= bits;
            self.write(offset, value);
        }
    }

    /// Reads a value from the MMIO address at the specified offset and clears some of its bits.
    ///
    /// # Panics
    ///
    /// This function will panic if either the read or write operation fails.
    #[inline]
    pub unsafe fn clear(&mut self, offset: usize, bits: T) {
        unsafe {
            let mut value = self.read(offset);
            value &= !bits;
            self.write(offset, value);
        }
    }

    /// Reads a value from the MMIO address at the specified offset and sets some of its bits, asserting that the value was written correctly.
    ///
    /// # Panics
    ///
    /// This function will panic if the read value does not match the expected value after writing,
    /// or if the read or write operations fail.
    #[inline]
    #[track_caller]
    pub unsafe fn set_assert(&mut self, offset: usize, bits: T) {
        unsafe {
            let mut value = self.read(offset);
            value |= bits;
            self.write_assert(offset, value);
        }
    }

    /// Reads a value from the MMIO address at the specified offset and clears some of its bits, asserting that the value was written correctly.
    ///
    /// # Panics
    ///
    /// This function will panic if the read value does not match the expected value after writing,
    /// or if the read or write operations fail.
    #[inline]
    #[track_caller]
    pub unsafe fn clear_assert(&mut self, offset: usize, bits: T) {
        unsafe {
            let mut value = self.read(offset);
            value &= !bits;
            self.write_assert(offset, value);
        }
    }

    #[inline]
    pub unsafe fn spin_until_hi(&self, offset: usize, mask: T) {
        crate::util::spin_while(|| unsafe { self.read(offset) & mask != mask });
    }

    #[inline]
    pub unsafe fn spin_while_hi(&self, offset: usize, mask: T) {
        crate::util::spin_while(|| unsafe { self.read(offset) & mask == mask });
    }

    #[inline]
    pub unsafe fn spin_until_lo(&self, offset: usize, mask: T) {
        crate::util::spin_while(|| unsafe { self.read(offset) & mask != T::ZERO });
    }

    #[inline]
    pub unsafe fn spin_while_lo(&self, offset: usize, mask: T) {
        crate::util::spin_while(|| unsafe { self.read(offset) & mask == T::ZERO });
    }
}
