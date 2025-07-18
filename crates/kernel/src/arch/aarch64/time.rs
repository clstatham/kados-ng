use core::time::Duration;

use aarch64_cpu::{
    asm::barrier,
    registers::{
        CNTFRQ_EL0, CNTP_CTL_EL0, CNTP_TVAL_EL0, CNTPCT_EL0, ReadWriteable, Readable, Writeable,
    },
};
use fdt::Fdt;

use crate::{
    irq::{Irq, IrqHandler, register_irq},
    task::switch::switch,
};

/// Initializes the generic timer for the `AArch64` architecture.
pub fn init(_fdt: &Fdt) {
    let mut timer = GenericTimer::default();
    timer.init();

    let irq = Irq::from(30);
    unsafe { register_irq(irq, timer) };
}

/// The generic timer for the `AArch64` architecture.
#[derive(Debug, Default)]
pub struct GenericTimer {
    pub clk_freq: u32,
    pub reload_count: u32,
}

impl GenericTimer {
    /// Initializes the generic timer with the current clock frequency.
    pub fn init(&mut self) {
        let clk_freq = CNTFRQ_EL0.get();
        self.clk_freq = clk_freq as u32;
        self.reload_count = clk_freq as u32 / 100;

        CNTP_TVAL_EL0.set(u64::from(self.reload_count));

        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::ENABLE::SET);
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::CLEAR);
    }

    /// Clears the interrupt status for the generic timer.
    pub fn clear_irq(&mut self) {
        if CNTP_CTL_EL0.is_set(CNTP_CTL_EL0::ISTATUS) {
            CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::SET);
        }
    }

    /// Reads the current count value of the generic timer.
    pub fn reload_count(&mut self) {
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::ENABLE::SET);
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::CLEAR);
        CNTP_TVAL_EL0.set(u64::from(self.reload_count));
    }
}

impl IrqHandler for GenericTimer {
    fn handle_irq(&mut self, _irq: Irq) {
        self.clear_irq();
        switch();
        self.reload_count();
    }
}

/// Returns the current uptime of the system.
#[must_use]
pub fn uptime() -> Duration {
    barrier::isb(barrier::SY);
    let ticks = CNTPCT_EL0.get();
    let clk_freq = CNTFRQ_EL0.get();

    let secs = ticks / clk_freq;
    let sub_seconds = ticks % clk_freq;
    let nanos = (sub_seconds * 1_000_000_000 / clk_freq) as u32;

    Duration::new(secs as u64, nanos)
}

/// Spins for the specified duration, busy-waiting until the duration has elapsed.
#[inline]
pub fn spin_for(dur: Duration) {
    let stamp = uptime();
    crate::util::spin_while(|| uptime() - stamp < dur);
}
