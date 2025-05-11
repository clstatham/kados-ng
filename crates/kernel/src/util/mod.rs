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
