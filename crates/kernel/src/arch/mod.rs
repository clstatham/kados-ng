pub mod driver;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

#[cfg(target_arch = "aarch64")]
pub use self::aarch64::*;

pub struct Arch {
    pub driver_manager: driver::DriverManager,
}
