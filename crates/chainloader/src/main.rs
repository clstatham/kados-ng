#![no_std]
#![no_main]
#![allow(clippy::identity_op)]

use core::{
    arch::{asm, global_asm},
    panic::PanicInfo,
};

global_asm!(include_str!("start.S"));

const KERNEL_LOAD_ADDR: usize = 0x80000;

const PERIPHERAL_BASE: usize = 0xFE00_0000;
const GPIO_BASE: usize = PERIPHERAL_BASE + 0x20_0000;
const UART0_BASE: usize = PERIPHERAL_BASE + 0x20_1000;

const GPFSEL1: *mut u32 = (GPIO_BASE + 0x04) as *mut u32;
const GPPUD: *mut u32 = (GPIO_BASE + 0x94) as *mut u32;
const GPPUDCLK0: *mut u32 = (GPIO_BASE + 0x98) as *mut u32;

const UART0_DR: *mut u32 = (UART0_BASE + 0x00) as *mut u32;
const UART0_FR: *mut u32 = (UART0_BASE + 0x18) as *mut u32;
const UART0_IBRD: *mut u32 = (UART0_BASE + 0x24) as *mut u32;
const UART0_FBRD: *mut u32 = (UART0_BASE + 0x28) as *mut u32;
const UART0_LCRH: *mut u32 = (UART0_BASE + 0x2C) as *mut u32;
const UART0_CR: *mut u32 = (UART0_BASE + 0x30) as *mut u32;
const UART0_ICR: *mut u32 = (UART0_BASE + 0x44) as *mut u32;

const AUX_ENABLE: *mut u32 = (PERIPHERAL_BASE + 0x00215004) as *mut u32;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe {
            asm!("wfe");
        }
    }
}

pub fn putchar(c: u8) {
    unsafe {
        while UART0_FR.read_volatile() & 0x20 != 0 {
            asm!("nop");
        }
        UART0_DR.write_volatile(c as u32);
    }
}

pub fn getchar() -> u8 {
    unsafe {
        while UART0_FR.read_volatile() & 0x10 != 0 {
            asm!("nop");
        }
        UART0_DR.read_volatile() as u8
    }
}

pub fn delay(mut cnt: usize) {
    unsafe {
        while cnt != 0 {
            asm!("nop");
            cnt -= 1;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn recv(_load_addr: usize) -> ! {
    unsafe {
        UART0_CR.write_volatile(0);
        AUX_ENABLE.write_volatile(0);
        let mut r = GPFSEL1.read_volatile();
        r &= !((7 << 12) | (7 << 15));
        r |= (4 << 12) | (4 << 15);
        GPFSEL1.write_volatile(r);
        GPPUD.write_volatile(0);
        delay(150);
        GPPUDCLK0.write_volatile((1 << 14) | (1 << 15));
        delay(150);
        GPPUDCLK0.write_volatile(0);

        UART0_ICR.write_volatile(0x7ff);
        UART0_IBRD.write_volatile(3);
        UART0_FBRD.write_volatile(16);
        UART0_LCRH.write_volatile(0x3 << 5);
        UART0_CR.write_volatile(0x301);
    }

    putchar(3);
    putchar(3);
    putchar(3);

    let mut kernel_len: u32 = 0;
    kernel_len |= getchar() as u32;
    kernel_len |= (getchar() as u32) << 8;
    kernel_len |= (getchar() as u32) << 16;
    kernel_len |= (getchar() as u32) << 24;

    putchar(b'O');
    putchar(b'K');

    unsafe {
        let mut i: usize = 0;
        while i < kernel_len as usize {
            let c = getchar();
            ((KERNEL_LOAD_ADDR + i) as *mut u8).write_volatile(c);
            putchar(c);
            i += 1;
        }
    }

    putchar(b'T');
    putchar(b'Y');
    putchar(b':');
    putchar(b')');

    unsafe { asm!("mov x0, x20", "br {}", in(reg) KERNEL_LOAD_ADDR, options(noreturn)) }
}
