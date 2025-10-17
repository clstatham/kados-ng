use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::{collections::btree_set::BTreeSet, sync::Arc};
use derive_more::{Deref, Display};
use spin::RwLock;
use spinning_top::RwSpinlock;

use crate::{
    arch::task::ArchContext, cpu_local::CpuLocalBlock,
    mem::paging::allocator::KernelFrameAllocator, syscall::errno::Errno,
};

use super::{addr_space::AddrSpaceLock, stack::Stack, switch::EMPTY_TABLE};

pub static CONTEXTS: RwLock<BTreeSet<ContextRef>> = RwLock::new(BTreeSet::new());

/// Initializes the kernel context.
///
/// # Panics
///
/// This function will panic if the kernel context cannot be created or if the frame allocator fails to allocate a frame.
pub fn init() {
    let mut cx = Context::new().expect("Failed to create kernel_main context");

    EMPTY_TABLE.call_once(|| unsafe { KernelFrameAllocator.allocate_one().unwrap() });

    cx.status = Status::Runnable;
    cx.running = true;
    let cx_lock = Arc::new(RwSpinlock::new(cx));
    CONTEXTS.write().insert(ContextRef(cx_lock.clone()));

    let block = CpuLocalBlock::current().unwrap();
    block.switch_state.set_current_context(cx_lock.clone());
    block.switch_state.set_idle_context(cx_lock);
}

#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    Runnable,
    Waiting,
    Blocked { reason: BlockReason },
    Dead,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BlockReason {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Display)]
pub struct Pid(usize);

impl Pid {
    pub fn alloc() -> Self {
        static NEXT_PID: AtomicUsize = AtomicUsize::new(0);
        Self(NEXT_PID.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct Context {
    pub status: Status,
    pub running: bool,
    pub arch: ArchContext,
    pub kstack: Option<Stack>,
    pub addr_space: Option<Arc<AddrSpaceLock>>,
    pub userspace: bool,
    pub pid: Pid,
}

impl Context {
    pub fn new() -> Result<Context, Errno> {
        Ok(Self {
            status: Status::Waiting,
            running: false,
            arch: ArchContext::default(),
            kstack: None,
            addr_space: None,
            userspace: false,
            pid: Pid::alloc(),
        })
    }
}

#[derive(Deref, Clone)]
pub struct ContextRef(pub Arc<RwSpinlock<Context>>);

impl PartialEq for ContextRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(self, other)
    }
}

impl Eq for ContextRef {}

impl Ord for ContextRef {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        Arc::as_ptr(self).cmp(&Arc::as_ptr(other))
    }
}

impl PartialOrd for ContextRef {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(Ord::cmp(self, other))
    }
}

#[must_use]
pub fn current() -> Option<Arc<RwSpinlock<Context>>> {
    CpuLocalBlock::current()
        .and_then(|block| block.switch_state.with_context(|cx| cx.map(Arc::clone)))
}

pub fn is_current(cx: &Arc<RwSpinlock<Context>>) -> bool {
    CpuLocalBlock::current().is_some_and(|block| {
        block
            .switch_state
            .with_context(|cur| cur.is_some_and(|cur| Arc::ptr_eq(cx, cur)))
    })
}

pub fn exit(cx: &Arc<RwSpinlock<Context>>) {
    CONTEXTS.write().remove(&ContextRef(cx.clone()));
    super::switch::switch();
    unreachable!()
}

pub fn exit_current() {
    if let Some(current) = current() {
        exit(&current);
    }
}
