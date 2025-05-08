#![no_std]
#![no_main]

use core::panic::PanicInfo;

use serial::GpioUart;

pub mod serial;

const KERNEL_LOAD_ADDR: usize = 0x80000;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("PANIC: {info}");
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(dtb_phys: usize) -> ! {
    serial::init();

    println!("Waiting for kernel...");

    for _ in 0..1000 {}

    for _ in 0..3 {
        GpioUart::putchar(3);
    }

    let mut len_bytes = [0u8; 4];
    for b in &mut len_bytes {
        *b = GpioUart::getchar();
    }
    let kernel_len = u32::from_be_bytes(len_bytes) as usize;
    println!("Receiving {kernel_len} bytes...");

    let mut dst = KERNEL_LOAD_ADDR as *mut u8;
    unsafe {
        for _ in 0..kernel_len {
            let b = GpioUart::getchar();
            dst.write_volatile(b);
            dst = dst.add(1);
        }
    }

    let mut csum_bytes = [0u8; 4];
    for b in &mut csum_bytes {
        *b = GpioUart::getchar();
    }
    let sent_checksum = u32::from_be_bytes(csum_bytes);
    let calc_checksum = const_crc32_nostd::crc32(unsafe {
        core::slice::from_raw_parts(KERNEL_LOAD_ADDR as *const u8, kernel_len)
    });
    if sent_checksum != calc_checksum {
        panic!("Checksum mismatch!");
    }

    println!("Jumping to kernel...");
    let kernel_entry: extern "C" fn(usize) -> ! = unsafe { core::mem::transmute(KERNEL_LOAD_ADDR) };

    kernel_entry(dtb_phys)
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        let _ = $crate::serial::GpioUart.write_fmt(format_args!($($arg)*));
        let _ = $crate::serial::GpioUart.write_fmt(format_args!("\n"));
    });
}
