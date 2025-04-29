#![no_std]
#![no_main]
#![allow(internal_features)]
#![feature(lang_items, test)]

#[macro_use]
pub mod serial;

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

#[panic_handler]
unsafe fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("Panic: {}", info);
    arch::exit_qemu(1)
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    arch::serial::init();
    println!("Hello, Kados!");
    arch::halt_loop();
}
