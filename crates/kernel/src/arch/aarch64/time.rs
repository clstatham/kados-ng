use core::time::Duration;

use aarch64_cpu::{asm::barrier, registers::*};
use fdt::Fdt;

use crate::{
    dtb::{Irq, IrqHandlerTrait, irq_chip, register_irq},
    mem::units::VirtAddr,
    task::switch::switch,
};

use super::mmio::Mmio;

pub fn init(_fdt: &Fdt) {
    let mut timer = GenericTimer::default();
    timer.init();

    let irq = Irq(30);
    unsafe { register_irq(irq, timer) };
    unsafe { irq_chip().enable_irq(irq) };
}

pub struct SystemTimer {
    pub base: Mmio<u32>,
    pub interval_micros: u32,
}

impl SystemTimer {
    pub const CS: usize = 0x00;
    pub const CLO: usize = 0x04;
    pub const C1: usize = 0x10;

    pub fn new(base: VirtAddr, interval_micros: u32) -> Self {
        Self {
            base: Mmio::new(base),
            interval_micros,
        }
    }

    pub fn init(&mut self) {
        unsafe {
            let now = self.base.read(Self::CLO);
            self.base
                .write_assert(Self::C1, now.wrapping_add(self.interval_micros));
            self.base.write(Self::CS, 1 << 1);
        }
    }
}

impl IrqHandlerTrait for SystemTimer {
    fn handle_irq(&mut self, _irq: Irq) {
        *crate::time::UPTIME.lock() += self.interval_micros as u64;
        switch();

        unsafe {
            self.base.write(Self::CS, 1 << 1);
            self.base.write_assert(
                Self::C1,
                self.base.read(Self::C1).wrapping_add(self.interval_micros),
            );
        }
    }
}

#[derive(Debug, Default)]
pub struct GenericTimer {
    pub clk_freq: u32,
    pub reload_count: u32,
}

impl GenericTimer {
    pub fn init(&mut self) {
        let clk_freq = CNTFRQ_EL0.get();
        self.clk_freq = clk_freq as u32;
        self.reload_count = clk_freq as u32 / 100;

        CNTP_TVAL_EL0.set(self.reload_count as u64);

        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::ENABLE::SET);
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::CLEAR);
    }

    pub fn clear_irq(&mut self) {
        if CNTP_CTL_EL0.is_set(CNTP_CTL_EL0::ISTATUS) {
            CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::SET);
        }
    }

    pub fn reload_count(&mut self) {
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::ENABLE::SET);
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::CLEAR);
        CNTP_TVAL_EL0.set(self.reload_count as u64)
    }
}

impl IrqHandlerTrait for GenericTimer {
    fn handle_irq(&mut self, _irq: Irq) {
        self.clear_irq();
        *crate::time::UPTIME.lock() += self.clk_freq as u64;
        switch();
        self.reload_count();
    }
}

pub fn uptime() -> Duration {
    barrier::isb(barrier::SY);
    let ticks = CNTPCT_EL0.get();
    let clk_freq = CNTFRQ_EL0.get();

    let secs = ticks / clk_freq;
    let sub_seconds = ticks % clk_freq;
    let nanos = (unsafe { sub_seconds.unchecked_mul(1_000_000_000) } / clk_freq) as u32;

    Duration::new(secs as u64, nanos)
}
