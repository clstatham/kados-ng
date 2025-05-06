use core::cell::{Cell, RefCell};

use alloc::sync::Arc;

use crate::{
    arch::{Arch, ArchTrait},
    task::{addr_space::AddrSpaceLock, switch::CpuLocalSwitchState},
};

pub struct CpuLocalBlock {
    pub switch_state: CpuLocalSwitchState,

    pub current_addr_space: RefCell<Option<Arc<AddrSpaceLock>>>,
    pub next_addr_space: Cell<Option<Arc<AddrSpaceLock>>>,
}

impl CpuLocalBlock {
    pub fn init() -> Self {
        Self {
            switch_state: CpuLocalSwitchState::default(),
            current_addr_space: RefCell::new(None),
            next_addr_space: Cell::new(None),
        }
    }

    pub fn current() -> Option<&'static Self> {
        unsafe { Arch::current_cpu_local_block().deref().ok() }
    }
}
