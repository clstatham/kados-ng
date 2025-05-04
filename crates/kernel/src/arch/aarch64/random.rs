use rand::{
    Rng, SeedableRng,
    distr::{Distribution, StandardUniform},
};
use rand_chacha::ChaChaRng;
use spin::Once;

use crate::sync::IrqMutex;

use super::time::uptime;

static RNG: Once<IrqMutex<ChaChaRng>> = Once::new();

pub fn rng() -> &'static IrqMutex<ChaChaRng> {
    let rng = rand_chacha::ChaChaRng::seed_from_u64(uptime().as_nanos() as u64);
    RNG.call_once(|| IrqMutex::new(rng))
}

pub fn getrandom<T>() -> T
where
    StandardUniform: Distribution<T>,
{
    rng().lock().random::<T>()
}
