use addr_space::AddrSpaceLock;
use alloc::sync::Arc;
use context::{CONTEXTS, Context, ContextRef};
use spinning_top::RwSpinlock;
use stack::Stack;

use crate::syscall::errno::Errno;

pub mod addr_space;
pub mod context;
pub mod stack;
pub mod switch;

pub fn spawn(user: bool, entry_func: extern "C" fn()) -> Result<Arc<RwSpinlock<Context>>, Errno> {
    let stack = Stack::new()?;

    let cx_lock = Arc::new(RwSpinlock::new(Context::new()?));

    CONTEXTS.write().insert(ContextRef(cx_lock.clone()));

    {
        let mut cx = cx_lock.write();
        let addr_space = if user {
            AddrSpaceLock::new()?
        } else {
            AddrSpaceLock::kernel()?
        };
        let _ = cx.addr_space.replace(addr_space);
        cx.arch.setup_initial_call(&stack, entry_func, user);

        cx.kstack = Some(stack);
        cx.userspace = user;
    }

    Ok(cx_lock)
}
