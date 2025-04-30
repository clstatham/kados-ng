#![no_std]
#![no_main]
#![allow(internal_features, clippy::missing_safety_doc)]
#![feature(lang_items, test, custom_test_frameworks)]
#![test_runner(crate::testing::test_runner)]
#![reexport_test_harness_main = "test_main"]

pub mod arch;
pub mod logging;
#[macro_use]
pub mod serial;
pub mod panicking;
#[cfg(test)]
pub mod testing;

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    arch::driver::register_driver(&serial::UartDriver, "PL011 UART")
        .expect("Failed to register UART driver");

    unsafe {
        arch::driver::init_drivers().expect("Failed to initialize drivers");
    }

    logging::init();

    #[cfg(test)]
    test_main();

    log::info!("Kernel starting...");

    log::info!("Kernel started");
    arch::halt_loop()
}

#[cfg(test)]
mod tests {
    #[test_case]
    #[allow(clippy::eq_op)]
    fn test_tests() {
        assert_eq!(1 + 1, 2);
    }
}
