use core::time::Duration;

use aarch64_cpu::{asm::barrier, registers::*};
use fdt::Fdt;

use crate::{
    irq::{Irq, IrqHandler, register_irq},
    task::switch::switch,
};

pub fn init(_fdt: &Fdt) {
    let mut timer = GenericTimer::default();
    timer.init();

    let irq = Irq::from(30);
    unsafe { register_irq(irq, timer) };
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

impl IrqHandler for GenericTimer {
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
    let nanos = (sub_seconds * 1_000_000_000 / clk_freq) as u32;

    Duration::new(secs as u64, nanos)
}

#[inline(always)]
pub fn spin_for(dur: Duration) {
    let stamp = uptime();
    crate::util::spin_while(|| uptime() - stamp < dur);
}
