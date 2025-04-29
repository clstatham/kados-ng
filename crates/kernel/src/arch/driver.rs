use core::sync::atomic::AtomicUsize;

use spin::mutex::SpinMutex;
use thiserror::Error;

use crate::println;

pub const MAX_DRIVERS: usize = 32;

static DRIVER_MANAGER: DriverManager = DriverManager::new();

pub fn driver_manager() -> &'static DriverManager {
    &DRIVER_MANAGER
}

pub fn register_driver<T: Driver>(
    driver: &'static T,
    name: &'static str,
    post_init: Option<DriverInitFn>,
) -> Result<(), &'static str> {
    let descriptor = DriverDescriptor::new(driver, name, post_init);
    driver_manager().register_driver(descriptor)
}

pub fn register_drivers(drivers: &[DriverDescriptor]) -> Result<(), &'static str> {
    for descriptor in drivers {
        driver_manager().register_driver(*descriptor)?;
    }
    Ok(())
}

pub unsafe fn init_drivers() -> Result<(), DriverError> {
    unsafe { driver_manager().init_drivers() }
}

#[derive(Debug, Error)]
#[error("Driver error: {driver_name}/{instance_name}: {message}")]
pub struct DriverError {
    pub driver_name: &'static str,
    pub instance_name: &'static str,
    pub message: &'static str,
}

pub trait Driver: Sync {
    unsafe fn init(&self) -> Result<(), &'static str>;
    fn name(&self) -> &'static str;
}

pub type DriverInitFn = unsafe fn() -> Result<(), &'static str>;

#[derive(Clone, Copy)]
pub struct DriverDescriptor {
    driver: &'static dyn Driver,
    instance_name: &'static str,
    post_init: Option<DriverInitFn>,
}

impl DriverDescriptor {
    pub const fn new(
        driver: &'static dyn Driver,
        instance_name: &'static str,
        post_init: Option<DriverInitFn>,
    ) -> Self {
        Self {
            driver,
            instance_name,
            post_init,
        }
    }

    pub unsafe fn init(&self) -> Result<(), DriverError> {
        unsafe {
            if let Err(msg) = self.driver.init() {
                return Err(DriverError {
                    driver_name: self.driver.name(),
                    instance_name: self.instance_name,
                    message: msg,
                });
            }
        }
        if let Some(post_init) = self.post_init {
            unsafe {
                if let Err(msg) = post_init() {
                    return Err(DriverError {
                        driver_name: self.driver.name(),
                        instance_name: self.instance_name,
                        message: msg,
                    });
                }
            }
        }
        Ok(())
    }

    pub fn driver_name(&self) -> &'static str {
        self.driver.name()
    }

    pub fn instance_name(&self) -> &'static str {
        self.instance_name
    }
}

pub struct DriverManager {
    drivers: SpinMutex<[Option<DriverDescriptor>; MAX_DRIVERS]>,
    count: AtomicUsize,
}

impl DriverManager {
    const fn new() -> Self {
        Self {
            drivers: SpinMutex::new([None; MAX_DRIVERS]),
            count: AtomicUsize::new(0),
        }
    }

    pub fn driver_count(&self) -> usize {
        self.count.load(core::sync::atomic::Ordering::SeqCst)
    }

    pub fn register_driver(&self, driver: DriverDescriptor) -> Result<(), &'static str> {
        let count = self.driver_count();
        if count >= MAX_DRIVERS {
            return Err("DriverManager is full");
        }
        self.drivers.lock()[count] = Some(driver);
        self.count
            .fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    pub fn enumerate(&self) {
        println!("Registered drivers:");
        let count = self.driver_count();
        for i in 0..count {
            if let Some(driver) = self.drivers.lock()[i] {
                println!(
                    "    {}: {}/{}",
                    i,
                    driver.driver_name(),
                    driver.instance_name()
                );
            }
        }
    }

    pub unsafe fn init_drivers(&self) -> Result<(), DriverError> {
        let count = self.driver_count();
        for i in 0..count {
            if let Some(driver) = self.drivers.lock()[i] {
                unsafe {
                    driver.init()?;
                }
                println!(
                    "Driver \"{}/{}\" initialized",
                    driver.driver_name(),
                    driver.instance_name()
                );
            }
        }
        Ok(())
    }
}
