use core::{
    cell::{Cell, RefCell},
    ops::{Bound, Deref},
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::sync::Arc;
use spin::Once;
use spinning_top::{RwSpinlock, guard::ArcRwSpinlockWriteGuard};

use crate::{
    arch::{Arch, ArchTrait, task::switch_to},
    cpu_local::CpuLocalBlock,
    mem::units::PhysAddr,
    task::context::Status,
};

use super::context::{CONTEXTS, Context, ContextRef, current};

pub static SWITCH_LOCK: AtomicBool = AtomicBool::new(false);

pub static EMPTY_CR3: Once<PhysAddr> = Once::new();
pub fn empty_cr3() -> PhysAddr {
    *EMPTY_CR3.get().unwrap()
}

pub enum SwitchResult {
    Switched,
    AllIdle,
}

struct SwitchResultGuard {
    _prev: ArcRwSpinlockWriteGuard<Context>,
    _next: ArcRwSpinlockWriteGuard<Context>,
}

#[derive(Default)]
pub struct CpuLocalSwitchState {
    result: Cell<Option<SwitchResultGuard>>,
    current_context: RefCell<Option<Arc<RwSpinlock<Context>>>>,
    idle_context: RefCell<Option<Arc<RwSpinlock<Context>>>>,
}

impl CpuLocalSwitchState {
    pub fn with_context<R>(&self, f: impl FnOnce(Option<&Arc<RwSpinlock<Context>>>) -> R) -> R {
        f(self.current_context.borrow().as_ref())
    }

    pub fn set_current_context(&self, new_cx: Arc<RwSpinlock<Context>>) {
        *self.current_context.borrow_mut() = Some(new_cx);
    }

    pub fn set_idle_context(&self, new_cx: Arc<RwSpinlock<Context>>) {
        *self.idle_context.borrow_mut() = Some(new_cx);
    }

    pub fn idle_context(&self) -> Arc<RwSpinlock<Context>> {
        self.idle_context
            .borrow()
            .as_ref()
            .expect("No idle context")
            .clone()
    }
}

pub unsafe extern "C" fn switch_finish_hook() {
    if let Some(guards) = CpuLocalBlock::current().unwrap().switch_state.result.take() {
        drop(guards);
    } else {
        unreachable!();
    }

    SWITCH_LOCK.store(false, Ordering::SeqCst);

    unsafe {
        switch_arch_hook();
    }
}

pub unsafe fn switch_arch_hook() {
    let block = CpuLocalBlock::current().unwrap();

    let current_addr_space = block.current_addr_space.borrow();
    let next_addr_space = block.next_addr_space.take();

    let is_same = match (&*current_addr_space, &next_addr_space) {
        (Some(prev), Some(next)) => Arc::ptr_eq(prev, next),
        (Some(_), None) => false,
        (None, Some(_)) => false,
        (None, None) => true,
    };
    if is_same {
        return;
    }

    drop(current_addr_space);

    *block.current_addr_space.borrow_mut() = next_addr_space;
    if let Some(next) = &*block.current_addr_space.borrow() {
        let next = next.read();
        unsafe {
            next.table.make_current();
            Arch::invalidate_all();
        }
    }
}

fn is_runnable(cx: &mut Context) -> bool {
    if cx.running {
        return false;
    }

    if cx.status == Status::Waiting {
        cx.status = Status::Runnable;
    }

    matches!(cx.status, Status::Runnable)
}

pub fn switch() -> SwitchResult {
    let block = CpuLocalBlock::current().unwrap();

    while SWITCH_LOCK
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }

    let mut switch_state_opt = None;
    {
        let contexts = CONTEXTS.read();

        let prev_lock = current().unwrap();
        let prev_guard = prev_lock.write_arc();

        let idle = block.switch_state.idle_context();

        let mut skip_idle = true;
        for next_lock in contexts
            .range((
                Bound::Excluded(ContextRef(prev_lock.clone())),
                Bound::Unbounded,
            ))
            .chain(contexts.range((
                Bound::Unbounded,
                Bound::Excluded(ContextRef(prev_lock.clone())),
            )))
            .map(Deref::deref)
            .cloned()
            .chain(Some(Arc::clone(&idle)))
        {
            if Arc::ptr_eq(&next_lock, &idle) && skip_idle {
                skip_idle = false;
                continue;
            }

            let mut next_guard = next_lock.write_arc();
            if is_runnable(&mut next_guard) {
                switch_state_opt = Some((prev_guard, next_guard));
                break;
            }
        }
    }
    if let Some((mut prev_guard, mut next_guard)) = switch_state_opt {
        let mut prev_cx = &mut *prev_guard;
        let mut next_cx = &mut *next_guard;

        prev_cx.running = false;
        next_cx.running = true;

        block
            .switch_state
            .set_current_context(ArcRwSpinlockWriteGuard::rwlock(&next_guard).clone());

        unsafe {
            prev_cx = core::mem::transmute::<&'_ mut Context, &'_ mut Context>(&mut *prev_guard);
            next_cx = core::mem::transmute::<&'_ mut Context, &'_ mut Context>(&mut *next_guard);
        }

        block.switch_state.result.set(Some(SwitchResultGuard {
            _prev: prev_guard,
            _next: next_guard,
        }));

        unsafe {
            switch_to(prev_cx, next_cx);
        }

        SwitchResult::Switched
    } else {
        SWITCH_LOCK.store(false, Ordering::SeqCst);
        SwitchResult::AllIdle
    }
}
