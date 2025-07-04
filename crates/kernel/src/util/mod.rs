use core::fmt::{self, Debug};

#[macro_export]
macro_rules! int_wrapper {
    ($vis:vis $name:ident : $ty:ty) => {
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, derive_more::Display, derive_more::From, derive_more::Into, derive_more::LowerHex, derive_more::UpperHex, derive_more::Binary)]
        #[repr(transparent)]
        $vis struct $name ($ty);

        impl $name {
            #[inline(always)]
            pub const fn as_usize(self) -> usize {
                self.0 as usize
            }

            #[inline(always)]
            pub const fn from_usize(val: usize) -> Self {
                Self(val as $ty)
            }

            #[inline(always)]
            pub const fn value(self) -> $ty {
                self.0
            }
        }
    };
}

/// Busy-waits the current core until the provided function returns `false`.
#[inline(always)]
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
    fn debug_unwrap(self) -> Self;

    /// Checks if the value is `Ok` or `Some`, and returns it.
    ///
    /// If the value is `Err` or `None`, it panics in debug mode with a custom message.
    /// In release mode, it returns the `Err` or `None` value.
    fn debug_expect(self, msg: impl fmt::Display) -> Self;
}

impl<T, E: Debug> DebugPanic for Result<T, E> {
    #[inline(always)]
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

    #[inline(always)]
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

impl<T> DebugPanic for Option<T> {
    #[inline(always)]
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

    #[inline(always)]
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
