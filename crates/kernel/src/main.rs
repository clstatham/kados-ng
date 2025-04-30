#![no_std]
#![no_main]
#![allow(internal_features, clippy::missing_safety_doc)]
#![feature(lang_items, test, custom_test_frameworks)]
#![test_runner(crate::testing::test_runner)]
#![reexport_test_harness_main = "test_main"]

use limine::request::{
    DateAtBootRequest, EntryPointRequest, ExecutableFileRequest, HhdmRequest, MemoryMapRequest,
    StackSizeRequest,
};

pub mod arch;
pub mod logging;
#[macro_use]
pub mod serial;
pub mod panicking;
#[cfg(test)]
pub mod testing;

static HHDM: HhdmRequest = HhdmRequest::new();
static ENTRY_POINT: EntryPointRequest = EntryPointRequest::new().with_entry_point(kernel_main);
static _STACK: StackSizeRequest = StackSizeRequest::new().with_size(0x20000);
static BOOT_TIME: DateAtBootRequest = DateAtBootRequest::new();
static MEM_MAP: MemoryMapRequest = MemoryMapRequest::new();
static KRENEL_FILE: ExecutableFileRequest = ExecutableFileRequest::new();

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    unsafe {
        arch::time::init();
    }

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
