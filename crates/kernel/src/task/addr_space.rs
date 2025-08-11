use alloc::sync::Arc;
use spin::{RwLock, RwLockReadGuard, rwlock::RwLockWriteGuard};

use crate::{
    cpu_local::CpuLocalBlock,
    mem::paging::table::{PageTable, TableKind},
    syscall::errno::Errno,
};

pub struct AddrSpace {
    pub table: PageTable,
}

impl AddrSpace {
    /// Returns the current address space for the current CPU.
    pub fn current() -> Result<Arc<AddrSpaceLock>, Errno> {
        CpuLocalBlock::current()
            .ok_or(Errno::ESRCH)?
            .current_addr_space
            .borrow()
            .clone()
            .ok_or(Errno::ESRCH)
    }

    pub fn new_user() -> Result<Self, Errno> {
        Ok(Self {
            table: PageTable::create(TableKind::User),
        })
    }

    pub fn current_kernel() -> Result<Self, Errno> {
        Ok(Self {
            table: PageTable::current(TableKind::Kernel),
        })
    }
}

pub struct AddrSpaceLock {
    lock: RwLock<AddrSpace>,
}

impl AddrSpaceLock {
    pub fn new_user() -> Result<Arc<Self>, Errno> {
        let lock = RwLock::new(AddrSpace::new_user()?);
        Ok(Arc::new(Self { lock }))
    }

    pub fn current_kernel() -> Result<Arc<Self>, Errno> {
        let lock = RwLock::new(AddrSpace::current_kernel()?);
        Ok(Arc::new(Self { lock }))
    }

    pub fn read(&self) -> RwLockReadGuard<'_, AddrSpace> {
        self.lock.read()
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, AddrSpace> {
        self.lock.write()
    }
}
