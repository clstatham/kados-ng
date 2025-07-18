use core::fmt::{self, Debug};

use crate::{
    arch::{Arch, Architecture},
    println,
};

#[macro_export]
macro_rules! int_wrapper {
    ($vis:vis $name:ident : $ty:ty) => {
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, derive_more::Display, derive_more::From, derive_more::Into, derive_more::LowerHex, derive_more::UpperHex, derive_more::Binary)]
        #[repr(transparent)]
        $vis struct $name ($ty);

        impl $name {
            #[inline]
            #[must_use]
            pub const fn as_usize(self) -> usize {
                self.0 as usize
            }

            #[inline]
            #[must_use]
            pub const fn from_usize(val: usize) -> Self {
                Self(val as $ty)
            }

            #[inline]
            #[must_use]
            pub const fn value(self) -> $ty {
                self.0
            }
        }
    };
}

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
    /// If the value is `Err` or `None`, it panics in debug mode.
    /// In release mode, it returns the `Err` or `None` value.
    #[must_use]
    fn debug_unwrap(self) -> Self;

    /// Checks if the value is `Ok` or `Some`, and returns it.
    ///
    /// If the value is `Err` or `None`, it panics in debug mode with a custom message.
    /// In release mode, it returns the `Err` or `None` value.
    #[must_use]
    fn debug_expect(self, msg: impl fmt::Display) -> Self;
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
    fn debug_checked_expect(self, msg: impl fmt::Display) -> Self::Output;
}

impl<T, E: Debug> DebugPanic for Result<T, E> {
    #[inline]
    fn debug_unwrap(self) -> Self {
        match self {
            Ok(val) => Ok(val),
            Err(err) => {
                if cfg!(debug_assertions) {
                    panic!("DebugPanic: {:?}", err);
                } else {
                    Err(err)
                }
            }
        }
    }

    #[inline]
    fn debug_expect(self, msg: impl fmt::Display) -> Self {
        match self {
            Ok(val) => Ok(val),
            Err(err) => {
                if cfg!(debug_assertions) {
                    panic!("DebugPanic: {}: {:?}", msg, err);
                } else {
                    Err(err)
                }
            }
        }
    }
}

impl<T, E: Debug> DebugCheckedPanic for Result<T, E> {
    type Output = T;

    #[inline]
    fn debug_checked_unwrap(self) -> Self::Output {
        match self {
            Ok(val) => val,
            Err(err) => {
                if cfg!(debug_assertions) {
                    panic!("DebugCheckedPanic: {:?}", err);
                } else {
                    println!("DebugCheckedPanic: {:?}", err);
                    Arch::hcf();
                }
            }
        }
    }

    #[inline]
    fn debug_checked_expect(self, msg: impl fmt::Display) -> Self::Output {
        match self {
            Ok(val) => val,
            Err(err) => {
                if cfg!(debug_assertions) {
                    panic!("DebugCheckedPanic: {}: {:?}", msg, err);
                } else {
                    println!("DebugCheckedPanic: {}: {:?}", msg, err);
                    Arch::hcf();
                }
            }
        }
    }
}

impl<T> DebugPanic for Option<T> {
    #[inline]
    fn debug_unwrap(self) -> Self {
        match self {
            Some(val) => Some(val),
            None => {
                if cfg!(debug_assertions) {
                    panic!("DebugPanic: None");
                } else {
                    None
                }
            }
        }
    }

    #[inline]
    fn debug_expect(self, msg: impl fmt::Display) -> Self {
        match self {
            Some(val) => Some(val),
            None => {
                if cfg!(debug_assertions) {
                    panic!("DebugPanic: {}: None", msg);
                } else {
                    None
                }
            }
        }
    }
}

impl<T> DebugCheckedPanic for Option<T> {
    type Output = T;

    #[inline]
    fn debug_checked_unwrap(self) -> Self::Output {
        match self {
            Some(val) => val,
            None => {
                if cfg!(debug_assertions) {
                    panic!("DebugCheckedPanic: None");
                } else {
                    println!("DebugCheckedPanic: None");
                    Arch::hcf();
                }
            }
        }
    }

    #[inline]
    fn debug_checked_expect(self, msg: impl fmt::Display) -> Self::Output {
        match self {
            Some(val) => val,
            None => {
                if cfg!(debug_assertions) {
                    panic!("DebugCheckedPanic: {}: None", msg);
                } else {
                    println!("DebugCheckedPanic: {}: None", msg);
                    Arch::hcf();
                }
            }
        }
    }
}
