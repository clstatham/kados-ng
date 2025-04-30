use core::{
    num::{NonZeroU32, NonZeroU64, NonZeroU128},
    ops::{Add, Div},
    time::Duration,
};

use aarch64_cpu::{asm::barrier, registers::*};

const NANOSEC_PER_SEC: NonZeroU64 = NonZeroU64::new(1_000_000_000).unwrap();

#[unsafe(no_mangle)]
static ARCH_TIMER_COUNTER_FREQUENCY: NonZeroU32 = NonZeroU32::MIN;

fn arch_timer_counter_frequency() -> NonZeroU32 {
    unsafe { core::ptr::read_volatile(&ARCH_TIMER_COUNTER_FREQUENCY) }
}

pub unsafe fn init() {
    unsafe {
        core::arch::asm!(
            r#"ldr x1, =ARCH_TIMER_COUNTER_FREQUENCY
            mrs x2, CNTFRQ_EL0
            str w2, [x1]
            "#
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct GenericTimerValue {
    pub value: u64,
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
    pub fn new(value: u64) -> Self {
        Self { value }
    }
}

impl From<GenericTimerValue> for Duration {
    fn from(value: GenericTimerValue) -> Self {
        if value.value == 0 {
            Duration::ZERO
        } else {
            let frequency: NonZeroU64 = arch_timer_counter_frequency().into();

            let secs = value.value / frequency.get() as u64;
            let sub_seconds = value.value % frequency.get() as u64;
            let nanos =
                unsafe { sub_seconds.unchecked_mul(NANOSEC_PER_SEC.get()) }.div(frequency) as u32;

            Duration::new(secs, nanos)
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

        let counter_value =
            unsafe { duration.unchecked_mul(frequency) }.div(NonZeroU128::from(NANOSEC_PER_SEC));

        Ok(GenericTimerValue::new(counter_value as u64))
    }
}

fn max_duration() -> Duration {
    Duration::from(GenericTimerValue::new(u64::MAX))
}

pub fn resolution() -> Duration {
    Duration::from(GenericTimerValue::new(1))
}

#[inline(always)]
fn read_cntpct() -> GenericTimerValue {
    barrier::isb(barrier::SY);
    let cnt = CNTPCT_EL0.get();

    GenericTimerValue::new(cnt)
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

    while GenericTimerValue::new(CNTPCT_EL0.get()) < end {
        barrier::isb(barrier::SY);
    }
}
