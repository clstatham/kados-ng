use core::sync::atomic::AtomicUsize;

use derive_more::Deref;
use spin::mutex::SpinMutex;
use thiserror::Error;
use uuid::Uuid;

use crate::println;

use super::random::getrandom;

pub const MAX_DRIVERS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deref)]
pub struct DriverId(Uuid);

static DRIVER_MANAGER: DriverManager = DriverManager::new();

pub fn driver_manager() -> &'static DriverManager {
    &DRIVER_MANAGER
}

pub fn register_driver<T: Driver>(
    driver: &'static T,
    name: &'static str,
) -> Result<(), &'static str> {
    driver_manager().register_driver(driver, name)
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

#[derive(Clone, Copy)]
pub struct DriverDescriptor {
    driver: &'static dyn Driver,
    id: DriverId,
    instance_name: &'static str,
}

impl DriverDescriptor {
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

    pub fn register_driver(
        &self,
        driver: &'static dyn Driver,
        instance_name: &'static str,
    ) -> Result<(), &'static str> {
        let count = self.driver_count();
        if count >= MAX_DRIVERS {
            return Err("DriverManager is full");
        }
        let random_bytes = getrandom();
        let id = DriverId(uuid::Builder::from_random_bytes(random_bytes).into_uuid());
        let driver = DriverDescriptor {
            driver,
            id,
            instance_name,
        };
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
                    "    {:>2} ({}): {}/{}",
                    i,
                    driver.id.0,
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
                    "Driver {} ({}/{}) initialized",
                    driver.id.0,
                    driver.driver_name(),
                    driver.instance_name()
                );
            }
        }
        println!("All drivers initialized");
        self.enumerate();
        Ok(())
    }
}
