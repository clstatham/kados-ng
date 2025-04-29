use core::sync::atomic::{AtomicBool, Ordering};

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

fn prevent_double_panic() {
    static PANICKING: AtomicBool = AtomicBool::new(false);

    if PANICKING.swap(true, Ordering::SeqCst) {
        // Already panicking, avoid infinite loop
        crate::arch::exit_qemu(1);
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    prevent_double_panic();

    println!("Panic: {}", info);
    crate::arch::exit_qemu(1)
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    prevent_double_panic();

    println!("[failed]");
    println!("Panic: {}", info);
    crate::arch::exit_qemu(1)
}
