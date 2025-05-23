use core::{
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

use spin::mutex::{SpinMutex, SpinMutexGuard};
use thiserror::Error;

use crate::{
    arch::{Arch, Architecture},
    println,
};

pub struct SavedInterruptStatus(bool);

impl SavedInterruptStatus {
    pub fn save() -> Self {
        Self(unsafe { Arch::interrupts_enabled() })
    }

    pub fn enabled(&self) -> bool {
        self.0
    }
}

impl Drop for SavedInterruptStatus {
    fn drop(&mut self) {
        unsafe {
            Arch::set_interrupts_enabled(self.0);
        }
    }
}

#[derive(Debug, Error)]
#[error("Cannot relock mutex")]
pub struct TryLockError;

pub struct IrqMutex<T: ?Sized>(SpinMutex<T>);

impl<T> IrqMutex<T> {
    pub const fn new(value: T) -> Self {
        Self(SpinMutex::new(value))
    }
}

impl<T: ?Sized> IrqMutex<T> {
    pub fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }

    pub fn try_lock(&self) -> Result<IrqMutexGuard<'_, T>, TryLockError> {
        if self.0.is_locked() {
            Err(TryLockError) // todo: more verbose error message
        } else {
            Ok(self.lock())
        }
    }

    pub fn lock(&self) -> IrqMutexGuard<'_, T> {
        if self.0.is_locked() {
            println!(
                "WARNING: Tried to relock IrqMutex of {}",
                core::any::type_name::<T>()
            );
            crate::panicking::unwind_kernel_stack().ok();
        }

        let saved_intr_status = SavedInterruptStatus::save();
        unsafe {
            Arch::disable_interrupts();
        }

        let guard = self.0.lock();

        IrqMutexGuard {
            inner: ManuallyDrop::new(guard),
            saved_intr_status: ManuallyDrop::new(saved_intr_status),
        }
    }

    pub fn is_locked(&self) -> bool {
        self.0.is_locked()
    }

    /// # Safety
    /// See [`spin::mutex::SpinMutex::force_unlock()`]
    pub unsafe fn force_unlock(&self) {
        unsafe { self.0.force_unlock() };
    }
}

unsafe impl<T: ?Sized + Send> Send for IrqMutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for IrqMutex<T> {}

pub struct IrqMutexGuard<'a, T: ?Sized> {
    inner: ManuallyDrop<SpinMutexGuard<'a, T>>,
    saved_intr_status: ManuallyDrop<SavedInterruptStatus>,
}

impl<T: ?Sized> Drop for IrqMutexGuard<'_, T> {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.inner);
        }

        unsafe {
            ManuallyDrop::drop(&mut self.saved_intr_status);
        }
    }
}

impl<T: ?Sized> Deref for IrqMutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: ?Sized> DerefMut for IrqMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}
