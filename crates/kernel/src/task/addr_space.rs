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

    pub fn new() -> Result<Self, Errno> {
        Ok(Self {
            table: PageTable::create(TableKind::User),
        })
    }

    pub fn kernel() -> Result<Self, Errno> {
        Ok(Self {
            table: PageTable::current(TableKind::Kernel),
        })
    }
}

pub struct AddrSpaceLock {
    lock: RwLock<AddrSpace>,
}

impl AddrSpaceLock {
    pub fn new() -> Result<Arc<Self>, Errno> {
        let lock = RwLock::new(AddrSpace::new()?);
        Ok(Arc::new(Self { lock }))
    }

    pub fn kernel() -> Result<Arc<Self>, Errno> {
        let lock = RwLock::new(AddrSpace::kernel()?);
        Ok(Arc::new(Self { lock }))
    }

    pub fn read(&self) -> RwLockReadGuard<'_, AddrSpace> {
        self.lock.read()
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, AddrSpace> {
        self.lock.write()
    }
}
