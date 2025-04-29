#![no_std]
#![allow(clippy::missing_safety_doc)]

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

#[cfg(target_arch = "aarch64")]
pub use self::aarch64::*;
