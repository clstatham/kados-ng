#![allow(unused)]

use core::fmt::Debug;

use crate::println;

/// Busy-waits the current core until the provided function returns `false`.
#[inline]
pub fn spin_while(f: impl Fn() -> bool) {
    while f() {
        core::hint::spin_loop();
    }
}

/// A trait to provide debug-mode panic-on-error behavior for `Result` and `Option`.
///
/// This is useful for debugging purposes, as it allows you to catch errors in debug builds
/// while still allowing the program to continue running in release builds.
pub trait DebugPanic {
    /// Checks if the value is `Ok` or `Some`, and returns it.
    ///
    /// In debug mode, it panics if the value is `Err` or `None`.
    /// In release mode, it returns the `Err` or `None` value.
    #[must_use]
    fn debug_unwrap(self) -> Self;

    /// Checks if the value is `Ok` or `Some`, and returns it.
    ///
    /// In debug mode, it panics with a custom message if the value is `Err` or `None`.
    /// In release mode, it returns the `Err` or `None` value.
    #[must_use]
    fn debug_expect(self, msg: &str) -> Self;
}

pub trait DebugCheckedPanic: DebugPanic {
    type Output;

    /// Checks if the value is `Ok` or `Some`, and returns it.
    ///
    /// If the value is `Err` or `None`, it panics in debug mode, or ***invokes undefined behavior*** in release mode.
    fn debug_checked_unwrap(self) -> Self::Output;

    /// Checks if the value is `Ok` or `Some`, and returns it.
    ///
    /// If the value is `Err` or `None`, it panics in debug mode with a custom message, or ***invokes undefined behavior*** in release mode.
    fn debug_checked_expect(self, msg: &str) -> Self::Output;
}

impl<T, E: Debug> DebugPanic for Result<T, E> {
    #[inline]
    fn debug_unwrap(self) -> Self {
        #[cfg(debug_assertions)]
        {
            Ok(self.unwrap())
        }
        #[cfg(not(debug_assertions))]
        self
    }

    #[inline]
    fn debug_expect(self, msg: &str) -> Self {
        #[cfg(debug_assertions)]
        {
            Ok(self.expect(msg))
        }
        #[cfg(not(debug_assertions))]
        self
    }
}

impl<T, E: Debug> DebugCheckedPanic for Result<T, E> {
    type Output = T;

    #[inline]
    fn debug_checked_unwrap(self) -> Self::Output {
        #[cfg(debug_assertions)]
        {
            self.unwrap()
        }
        #[cfg(not(debug_assertions))]
        match self {
            Ok(v) => v,
            Err(e) => {
                println!("debug_checked_unwrap failed: {e:?}");
                unsafe { core::hint::unreachable_unchecked() }
            }
        }
    }

    #[inline]
    fn debug_checked_expect(self, msg: &str) -> Self::Output {
        #[cfg(debug_assertions)]
        {
            self.expect(msg)
        }
        #[cfg(not(debug_assertions))]
        match self {
            Ok(v) => v,
            Err(e) => {
                println!("debug_checked_expect failed: {msg}: {e:?}");
                unsafe { core::hint::unreachable_unchecked() }
            }
        }
    }
}

impl<T> DebugPanic for Option<T> {
    #[inline]
    fn debug_unwrap(self) -> Self {
        #[cfg(debug_assertions)]
        {
            Some(self.unwrap())
        }
        #[cfg(not(debug_assertions))]
        self
    }

    #[inline]
    fn debug_expect(self, msg: &str) -> Self {
        #[cfg(debug_assertions)]
        {
            Some(self.expect(msg))
        }
        #[cfg(not(debug_assertions))]
        self
    }
}

impl<T> DebugCheckedPanic for Option<T> {
    type Output = T;

    #[inline]
    fn debug_checked_unwrap(self) -> Self::Output {
        #[cfg(debug_assertions)]
        {
            self.unwrap()
        }
        #[cfg(not(debug_assertions))]
        match self {
            Some(v) => v,
            None => {
                println!("debug_checked_unwrap failed");
                unsafe { core::hint::unreachable_unchecked() }
            }
        }
    }

    #[inline]
    fn debug_checked_expect(self, msg: &str) -> Self::Output {
        #[cfg(debug_assertions)]
        {
            self.expect(msg)
        }
        #[cfg(not(debug_assertions))]
        match self {
            Some(v) => v,
            None => {
                println!("debug_checked_expect failed: {msg}");
                unsafe { core::hint::unreachable_unchecked() }
            }
        }
    }
}
