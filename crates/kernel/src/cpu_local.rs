use core::cell::{Cell, RefCell};

use alloc::sync::Arc;

use crate::{
    arch::{Arch, Architecture},
    task::{addr_space::AddrSpaceLock, switch::CpuLocalSwitchState},
};

/// A block of data that is unique to each CPU core.
pub struct CpuLocalBlock {
    pub switch_state: CpuLocalSwitchState,

    pub current_addr_space: RefCell<Option<Arc<AddrSpaceLock>>>,
    pub next_addr_space: Cell<Option<Arc<AddrSpaceLock>>>,
}

impl CpuLocalBlock {
    /// Initializes a new `CpuLocalBlock` for the current CPU core.
    pub fn init() -> Self {
        Self {
            switch_state: CpuLocalSwitchState::default(),
            current_addr_space: RefCell::new(None),
            next_addr_space: Cell::new(None),
        }
    }

    /// Returns a reference to the current CPU local block.
    pub fn current() -> Option<&'static Self> {
        unsafe { Arch::current_cpu_local_block().deref().ok() }
    }
}
