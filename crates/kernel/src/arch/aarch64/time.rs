use core::{ops::Div, time::Duration};

use aarch64_cpu::{asm::barrier, registers::*};
use spin::Mutex;

use crate::task::switch::switch;

pub static TIMER: Mutex<GenericTimer> = Mutex::new(GenericTimer {
    clk_freq: 0,
    reload_count: 0,
});

pub fn init() {
    TIMER.lock().init();
}

pub fn uptime() -> Duration {
    TIMER.lock().uptime()
}

#[derive(Default)]
pub struct GenericTimer {
    pub clk_freq: u32,
    pub reload_count: u32,
}

impl GenericTimer {
    pub fn init(&mut self) {
        let clk_freq = CNTFRQ_EL0.get() as u32;
        self.clk_freq = clk_freq;
        self.reload_count = clk_freq / 100;

        CNTP_TVAL_EL0.set(self.reload_count as u64);

        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::ENABLE::SET);
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::CLEAR);
    }

    pub fn uptime(&self) -> Duration {
        barrier::isb(barrier::SY);
        let ticks = CNTPCT_EL0.get();

        let secs = ticks / self.clk_freq as u64;
        let sub_seconds = ticks % self.clk_freq as u64;
        let nanos =
            unsafe { sub_seconds.unchecked_mul(1_000_000_000) }.div(self.clk_freq as u64) as u32;

        Duration::new(secs as u64, nanos)
    }

    pub fn set_irq(&mut self) {
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::CLEAR);
    }

    pub fn clear_irq(&mut self) {
        if CNTP_CTL_EL0.matches_all(CNTP_CTL_EL0::ISTATUS::SET) {
            CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::SET);
        }
    }

    pub fn reload_count(&mut self) {
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::ENABLE::SET);
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::CLEAR);
        CNTP_TVAL_EL0.set(self.reload_count as u64);
    }

    pub fn handle_irq(&mut self) {
        self.clear_irq();

        switch();

        self.reload_count();
    }
}
