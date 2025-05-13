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
    pub const fn new(addr: VirtAddr) -> Self {
        Self {
            addr,
            _marker: PhantomData,
        }
    }

    #[inline(always)]
    pub unsafe fn read(&self, offset: usize) -> T {
        unsafe {
            asm!("dsb sy", "isb");
            self.addr.add_bytes(offset).read_volatile().unwrap()
        }
    }

    #[inline(always)]
    pub unsafe fn write(&mut self, offset: usize, value: T) {
        unsafe {
            self.addr.add_bytes(offset).write_volatile(value).unwrap();
            asm!("dsb sy", "isb");
        }
    }

    #[inline(always)]
    #[track_caller]
    pub unsafe fn write_assert(&mut self, offset: usize, value: T) {
        unsafe {
            self.write(offset, value);
            assert_eq!(self.read(offset), value);
        }
    }

    #[inline(always)]
    pub unsafe fn set(&mut self, offset: usize, bits: T) {
        unsafe {
            let mut value = self.read(offset);
            value |= bits;
            self.write(offset, value);
        }
    }

    #[inline(always)]
    pub unsafe fn clear(&mut self, offset: usize, bits: T) {
        unsafe {
            let mut value = self.read(offset);
            value &= !bits;
            self.write(offset, value);
        }
    }

    #[inline(always)]
    #[track_caller]
    pub unsafe fn set_assert(&mut self, offset: usize, bits: T) {
        unsafe {
            let mut value = self.read(offset);
            value |= bits;
            self.write_assert(offset, value);
        }
    }

    #[inline(always)]
    #[track_caller]
    pub unsafe fn clear_assert(&mut self, offset: usize, bits: T) {
        unsafe {
            let mut value = self.read(offset);
            value &= !bits;
            self.write_assert(offset, value);
        }
    }

    #[inline(always)]
    pub unsafe fn spin_until_hi(&self, offset: usize, mask: T) {
        crate::util::spin_while(|| unsafe { self.read(offset) & mask != mask });
    }

    #[inline(always)]
    pub unsafe fn spin_while_hi(&self, offset: usize, mask: T) {
        crate::util::spin_while(|| unsafe { self.read(offset) & mask == mask });
    }

    #[inline(always)]
    pub unsafe fn spin_until_lo(&self, offset: usize, mask: T) {
        crate::util::spin_while(|| unsafe { self.read(offset) & mask != T::ZERO });
    }

    #[inline(always)]
    pub unsafe fn spin_while_lo(&self, offset: usize, mask: T) {
        crate::util::spin_while(|| unsafe { self.read(offset) & mask == T::ZERO });
    }
}
