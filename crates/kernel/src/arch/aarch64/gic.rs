use aarch64_cpu::registers::*;

pub const GICD_BASE: *mut u32 = 0xFF84_1000 as *mut u32;
pub const GICC_BASE: *mut u32 = 0xFF84_2000 as *mut u32;
const TIMER_IRQ: u32 = 30;

unsafe fn gicd_write(off: usize, val: u32) {
    unsafe {
        GICD_BASE.byte_add(off).write_volatile(val);
    }
}

unsafe fn gicd_read(off: usize) -> u32 {
    unsafe { GICD_BASE.byte_add(off).read_volatile() }
}

unsafe fn gicd_set(off: usize, bit_shift: u8) {
    unsafe {
        let mut val = gicd_read(off);
        val |= 1 << bit_shift as u32;
        gicd_write(off, val);
    }
}

unsafe fn gicc_write(off: usize, val: u32) {
    unsafe {
        GICC_BASE.byte_add(off).write_volatile(val);
    }
}

unsafe fn gicc_read(off: usize) -> u32 {
    unsafe { GICC_BASE.byte_add(off).read_volatile() }
}

pub fn irq_num() -> u32 {
    unsafe { gicc_read(0x00c) }
}

pub fn eoi(irq: u32) {
    unsafe {
        gicc_write(0x0010, irq);
    }
}

#[inline(never)]
pub unsafe fn init() {
    unsafe {
        // GICD

        gicd_write(0x000, 0);

        gicd_set(0x080, TIMER_IRQ as u8); // group-1NS

        let idx = (TIMER_IRQ / 4) * 4;
        let shift = (TIMER_IRQ % 4) * 8;
        let reg = idx as usize + 0x400;
        let mut pri = gicd_read(reg);
        pri &= !(0xff << shift);
        pri |= 0x80 << shift;
        gicd_write(reg, pri);

        core::arch::asm!("dsb sy; isb");

        let cfg_reg = 0xC00 + 4;
        let mut cfg = gicd_read(cfg_reg);
        cfg &= !(0b11 << ((TIMER_IRQ - 16) * 2));
        gicd_write(cfg_reg, cfg);

        gicd_set(0x100, TIMER_IRQ as u8);

        core::arch::asm!("dsb sy; isb");

        gicd_write(0x000, 0b11);

        core::arch::asm!("isb");

        // GICC

        gicc_write(0x000, 0); // disable
        core::arch::asm!("dsb sy; isb");
        gicc_write(0x004, 0xff); // prio mask
        gicc_write(0x008, 0); // binary point 0
        core::arch::asm!("dsb sy; isb");
        gicc_write(0x000, 1 << 1); // enable group 1
        core::arch::asm!("dsb sy; isb");

        // timer

        CNTP_TVAL_EL0.set(CNTFRQ_EL0.get() * 3 / 1_000_000);
        CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET);

        core::arch::asm!("dsb sy; isb");
    }
}
