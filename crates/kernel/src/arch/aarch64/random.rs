use rand::{
    Rng, SeedableRng,
    distr::{Distribution, StandardUniform},
};
use rand_chacha::ChaChaRng;
use spin::{Once, mutex::SpinMutex};

use super::time::uptime;

static RNG: Once<SpinMutex<ChaChaRng>> = Once::new();

pub fn rng() -> &'static SpinMutex<ChaChaRng> {
    let rng = rand_chacha::ChaChaRng::seed_from_u64(uptime().as_nanos() as u64);
    RNG.call_once(|| SpinMutex::new(rng))
}

pub fn getrandom<T>() -> T
where
    StandardUniform: Distribution<T>,
{
    rng().lock().random::<T>()
}
