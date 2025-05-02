pub trait Test {
    fn run(&self);
}

impl<T> Test for T
where
    T: Fn(),
{
    fn run(&self) {
        print!("{}...\t", core::any::type_name::<T>());
        (self)();
        println!("[ok]");
    }
}

#[cfg(test)]
pub fn test_runner(tests: &[&dyn Test]) {
    use crate::arch::{Arch, ArchTrait};

    log::info!("Running tests...");

    for test in tests {
        test.run();
    }

    Arch::exit_qemu(0);
}
