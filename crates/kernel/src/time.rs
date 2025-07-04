use core::time::Duration;

/// Represents the system uptime (time since boot).
pub fn uptime() -> Duration {
    crate::arch::time::uptime()
}
