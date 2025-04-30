#![no_std]
#![no_main]
#![allow(internal_features, clippy::missing_safety_doc)]
#![feature(lang_items, test, custom_test_frameworks)]
#![test_runner(crate::testing::test_runner)]
#![reexport_test_harness_main = "test_main"]

use limine::{
    memory_map::EntryType,
    request::{
        DateAtBootRequest, EntryPointRequest, ExecutableFileRequest, HhdmRequest, MemoryMapRequest,
        StackSizeRequest,
    },
};

pub mod arch;
pub mod logging;
#[macro_use]
pub mod serial;
pub mod mmu;
pub mod panicking;
#[cfg(test)]
pub mod testing;

static HHDM: HhdmRequest = HhdmRequest::new();
static _ENTRY_POINT: EntryPointRequest = EntryPointRequest::new().with_entry_point(kernel_main);
static _STACK: StackSizeRequest = StackSizeRequest::new().with_size(0x20000);
static BOOT_TIME: DateAtBootRequest = DateAtBootRequest::new();
static MEM_MAP: MemoryMapRequest = MemoryMapRequest::new();
// static KRENEL_FILE: ExecutableFileRequest = ExecutableFileRequest::new();

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    let hhdm = HHDM.get_response().unwrap();
    mmu::HDDM_PHYSICAL_OFFSET.call_once(|| hhdm.offset());

    let boot_time = BOOT_TIME.get_response().unwrap();

    unsafe {
        arch::time::init(boot_time.timestamp());
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

    let mem_map = MEM_MAP.get_response().unwrap();
    let mut total_free = 0;
    for entry in mem_map
        .entries()
        .iter()
        .filter(|e| e.entry_type == EntryType::USABLE)
    {
        log::info!(
            "usable region: {:016x} .. {:016x}",
            entry.base,
            entry.base + entry.length,
        );
        total_free += entry.length;
    }
    log::info!("{total_free} bytes free");

    log::info!("Kernel boot finished at {}", arch::time::Instant::now());

    log::info!("Welcome to KaDOS!");
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
