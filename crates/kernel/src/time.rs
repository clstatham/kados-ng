use core::time::Duration;

use spin::Mutex;

pub static UPTIME: Mutex<u64> = Mutex::new(0);

pub fn uptime() -> Duration {
    crate::arch::time::uptime()
}
