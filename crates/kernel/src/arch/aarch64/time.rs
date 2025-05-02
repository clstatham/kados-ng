use core::{
    fmt::Display,
    num::{NonZeroU32, NonZeroU128, NonZeroUsize},
    ops::{Add, Div},
    time::Duration,
};

use aarch64_cpu::{asm::barrier, registers::*};
use chrono_light::prelude::*;
use spin::Once;

const NANOSEC_PER_SEC: NonZeroUsize = NonZeroUsize::new(1_000_000_000).unwrap();

#[unsafe(no_mangle)]
static ARCH_TIMER_COUNTER_FREQUENCY: NonZeroU32 = NonZeroU32::MIN;

fn arch_timer_counter_frequency() -> NonZeroU32 {
    unsafe { core::ptr::read_volatile(&ARCH_TIMER_COUNTER_FREQUENCY) }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant(DateTime);

impl Instant {
    pub fn now() -> Self {
        let calendar = Calendar::create();
        Instant(calendar.from_unixtime(
            uptime().as_millis() as u64 + calendar.to_unixtime(&DATE_AT_BOOT.get().unwrap().0),
        ))
    }
}

impl Display for Instant {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}/{}/{} {}:{}:{}.{:03}",
            self.0.year,
            self.0.month,
            self.0.day,
            self.0.hour,
            self.0.minute,
            self.0.second,
            self.0.ms
        )
    }
}

static DATE_AT_BOOT: Once<Instant> = Once::new();

pub unsafe fn init(date_at_boot: Duration) {
    unsafe {
        core::arch::asm!(
            r#"
            ldr x1, =ARCH_TIMER_COUNTER_FREQUENCY
            mrs x2, CNTFRQ_EL0
            str w2, [x1]
            "#
        );
    }

    DATE_AT_BOOT
        .call_once(|| Instant(Calendar::create().from_unixtime(date_at_boot.as_millis() as u64)));
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct GenericTimerValue {
    pub value: usize,
}

impl Add for GenericTimerValue {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            value: self.value.wrapping_add(other.value),
        }
    }
}

impl GenericTimerValue {
    pub fn new(value: usize) -> Self {
        Self { value }
    }
}

impl From<GenericTimerValue> for Duration {
    fn from(value: GenericTimerValue) -> Self {
        if value.value == 0 {
            Duration::ZERO
        } else {
            let frequency = arch_timer_counter_frequency().get() as usize;

            let secs = value.value / frequency;
            let sub_seconds = value.value % frequency;
            let nanos =
                unsafe { sub_seconds.unchecked_mul(NANOSEC_PER_SEC.get()) }.div(frequency) as u32;

            Duration::new(secs as u64, nanos)
        }
    }
}

impl TryFrom<Duration> for GenericTimerValue {
    type Error = &'static str;

    fn try_from(value: Duration) -> Result<Self, Self::Error> {
        if value < resolution() {
            return Ok(Self::new(0));
        }

        if value > max_duration() {
            return Err("Duration exceeds maximum timer value");
        }

        let frequency = u32::from(arch_timer_counter_frequency()) as u128;
        let duration = value.as_nanos();

        let counter_value = unsafe { duration.unchecked_mul(frequency) }
            .div(NonZeroU128::new(NANOSEC_PER_SEC.get() as u128).unwrap());

        Ok(GenericTimerValue::new(counter_value as usize))
    }
}

fn max_duration() -> Duration {
    Duration::from(GenericTimerValue::new(usize::MAX))
}

pub fn resolution() -> Duration {
    Duration::from(GenericTimerValue::new(1))
}

#[inline(always)]
fn read_cntpct() -> GenericTimerValue {
    barrier::isb(barrier::SY);
    let cnt = CNTPCT_EL0.get();

    GenericTimerValue::new(cnt as usize)
}

pub fn uptime() -> Duration {
    read_cntpct().into()
}

pub fn spin_for(duration: Duration) {
    let start = read_cntpct();
    let delta = match duration.try_into() {
        Ok(delta) => delta,
        Err(e) => {
            log::warn!("Failed to convert duration: {e}");
            return;
        }
    };
    let end = start + delta;

    while GenericTimerValue::new(CNTPCT_EL0.get() as usize) < end {
        barrier::isb(barrier::SY);
    }
}
