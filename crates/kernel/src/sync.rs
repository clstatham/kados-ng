use core::{
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

use spin::mutex::{SpinMutex, SpinMutexGuard};
use thiserror::Error;

use crate::{
    arch::{Arch, Architecture},
    println,
};

/// A struct that saves the current interrupt status and restores it when dropped.
/// This is useful for ensuring that interrupts are disabled while a critical section is executed.
/// It is important to note that this struct should only be used in a single-threaded context.
/// Using it in a multi-threaded context may lead to undefined behavior.
#[must_use = "Interrupt status will be restored when this is dropped"]
#[derive(Debug)]
pub struct SavedInterruptStatus {
    /// The current interrupt status.
    /// `true` if interrupts are enabled, `false` otherwise.
    pub(crate) enabled: bool,
    /// A marker to indicate that this struct is not `Sync`.
    pub(crate) _marker: PhantomData<*const ()>,
}

impl SavedInterruptStatus {
    /// Saves the current interrupt status and returns a `SavedInterruptStatus` instance.
    /// This function should be called before entering a critical section.
    pub fn save() -> Self {
        Self {
            enabled: unsafe { Arch::interrupts_enabled() },
            _marker: PhantomData,
        }
    }

    /// Returns whether interrupts were enabled when this struct was created.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}

impl Drop for SavedInterruptStatus {
    fn drop(&mut self) {
        unsafe {
            Arch::set_interrupts_enabled(self.enabled);
        }
    }
}

/// An error that can occur when trying to lock an `IrqMutex` that is already locked.
///
/// This error indicates that the mutex is already held by another thread or interrupt handler.
///
/// It is important to note that this error should not occur in a single-threaded context.
/// If it does, it may indicate a bug in the code.
#[derive(Debug, Error)]
#[error("Cannot relock mutex")]
pub struct TryLockError;

/// A mutex that can be used in critical sections where interrupts need to be disabled.
pub struct IrqMutex<T: ?Sized>(SpinMutex<T>);

impl<T> IrqMutex<T> {
    /// Creates a new `IrqMutex` instance with the given inner value.
    pub const fn new(value: T) -> Self {
        Self(SpinMutex::new(value))
    }
}

impl<T: ?Sized> IrqMutex<T> {
    /// Returns a mutable reference to the inner value.
    ///
    /// This is safe because it requires a mutable reference to the `IrqMutex` itself.
    /// As such, no actual locking is performed here.
    pub fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }

    /// Attempts to lock the `IrqMutex` and returns a guard that can be used to access the inner value.
    ///
    /// This function will return an error if the mutex is already locked.
    /// This is useful for avoiding deadlocks in multi-threaded contexts.
    pub fn try_lock(&self) -> Result<IrqMutexGuard<'_, T>, TryLockError> {
        if self.0.is_locked() {
            Err(TryLockError) // todo: more verbose error message
        } else {
            Ok(self.lock())
        }
    }

    /// Locks the `IrqMutex` and returns a guard that can be used to access the inner value.
    ///
    /// This function will disable interrupts while the mutex is locked, and will restore the interrupt status when the guard is dropped.
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

    /// Returns `true` if the mutex is currently locked, `false` otherwise.
    pub fn is_locked(&self) -> bool {
        self.0.is_locked()
    }

    /// Force-unlocks the mutex without restoring the interrupt status.
    ///
    /// # Safety
    /// See [`spin::mutex::SpinMutex::force_unlock()`]
    pub unsafe fn force_unlock(&self) {
        unsafe { self.0.force_unlock() };
    }
}

// TODO: Are these needed, and are they safe?
// unsafe impl<T: ?Sized + Send> Send for IrqMutex<T> {}
// unsafe impl<T: ?Sized + Send> Sync for IrqMutex<T> {}

/// A guard that can be used to access the inner value of an `IrqMutex`.
///
/// This guard will unlock the mutex and restore the interrupt status when it is dropped.
#[derive(Debug)]
#[must_use = "Mutex will be unlocked and interrupt status will be restored when this is dropped"]
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
