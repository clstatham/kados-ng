use core::time::Duration;

/// Represents the system uptime (time since boot).
#[must_use]
pub fn uptime() -> Duration {
    crate::arch::time::uptime()
}
