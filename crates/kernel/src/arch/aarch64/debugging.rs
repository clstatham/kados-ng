use core::{
    arch::asm,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::collections::btree_map::BTreeMap;
use gdbstub::{
    arch::Arch,
    conn::{Connection, ConnectionExt},
    stub::{GdbStub, SingleThreadStopReason, state_machine::GdbStubStateMachine},
    target::{
        Target, TargetResult,
        ext::{
            base::{
                BaseOps,
                single_register_access::SingleRegisterAccessOps,
                singlethread::{SingleThreadBase, SingleThreadResume, SingleThreadResumeOps},
            },
            breakpoints::{Breakpoints, BreakpointsOps, SwBreakpoint, SwBreakpointOps},
        },
    },
};
use gdbstub_arch::aarch64::AArch64;

use crate::{arch::vectors::InterruptFrame, pop_preserved, pop_scratch, pop_special};

pub static GDB_ACTIVE: AtomicBool = AtomicBool::new(false);

#[inline(always)]
pub fn gdb_active() -> bool {
    GDB_ACTIVE.load(Ordering::SeqCst)
}

struct Serial;

impl Connection for Serial {
    type Error = ();
    fn write(&mut self, byte: u8) -> Result<(), Self::Error> {
        crate::arch::serial::GpioUart::putchar(byte);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn on_session_start(&mut self) -> Result<(), Self::Error> {
        log::debug!("Entering GDB stub");
        Ok(())
    }
}

impl ConnectionExt for Serial {
    fn read(&mut self) -> Result<u8, Self::Error> {
        Ok(crate::arch::serial::GpioUart::getchar())
    }

    fn peek(&mut self) -> Result<Option<u8>, Self::Error> {
        Ok(None)
    }
}

pub struct InterruptTarget<'a> {
    frame: &'a mut InterruptFrame,
    breakpoints: BTreeMap<u64, u64>,
}

impl Target for InterruptTarget<'_> {
    type Arch = AArch64;
    type Error = &'static str;

    fn base_ops(&mut self) -> BaseOps<'_, Self::Arch, Self::Error> {
        BaseOps::SingleThread(self)
    }

    fn support_breakpoints(&mut self) -> Option<BreakpointsOps<'_, Self>> {
        Some(self)
    }
}

impl Breakpoints for InterruptTarget<'_> {
    fn support_sw_breakpoint(&mut self) -> Option<SwBreakpointOps<'_, Self>> {
        Some(self)
    }
}

impl SwBreakpoint for InterruptTarget<'_> {
    fn add_sw_breakpoint(
        &mut self,
        addr: <Self::Arch as Arch>::Usize,
        _kind: <Self::Arch as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        unsafe {
            let original = (addr as *const u64).read_volatile();
            self.breakpoints.insert(addr, original);
            (addr as *mut u64).write_volatile(0xD43C0000); // brk #0xf000
        }
        Ok(true)
    }

    fn remove_sw_breakpoint(
        &mut self,
        addr: <Self::Arch as Arch>::Usize,
        _kind: <Self::Arch as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        unsafe {
            let original = self.breakpoints.remove(&addr).unwrap();
            (addr as *mut u64).write_volatile(original);
        }
        Ok(true)
    }
}

impl<'a> SingleThreadBase for InterruptTarget<'a> {
    #[inline(always)]
    fn read_registers(
        &mut self,
        regs: &mut <Self::Arch as Arch>::Registers,
    ) -> TargetResult<(), Self> {
        regs.pc = self.frame.instr_pointer() as u64;
        regs.sp = self.frame.stack_pointer() as u64;
        regs.x[0] = self.frame.scratch.x0 as u64;
        regs.x[1] = self.frame.scratch.x1 as u64;
        regs.x[2] = self.frame.scratch.x2 as u64;
        regs.x[3] = self.frame.scratch.x3 as u64;
        regs.x[4] = self.frame.scratch.x4 as u64;
        regs.x[5] = self.frame.scratch.x5 as u64;
        regs.x[6] = self.frame.scratch.x6 as u64;
        regs.x[7] = self.frame.scratch.x7 as u64;
        regs.x[8] = self.frame.scratch.x8 as u64;
        regs.x[9] = self.frame.scratch.x9 as u64;
        regs.x[10] = self.frame.scratch.x10 as u64;
        regs.x[11] = self.frame.scratch.x11 as u64;
        regs.x[12] = self.frame.scratch.x12 as u64;
        regs.x[13] = self.frame.scratch.x13 as u64;
        regs.x[14] = self.frame.scratch.x14 as u64;
        regs.x[15] = self.frame.scratch.x15 as u64;
        regs.x[16] = self.frame.scratch.x16 as u64;
        regs.x[17] = self.frame.scratch.x17 as u64;
        regs.x[18] = self.frame.scratch.x18 as u64;
        regs.x[19] = self.frame.preserved.x19 as u64;
        regs.x[20] = self.frame.preserved.x20 as u64;
        regs.x[21] = self.frame.preserved.x21 as u64;
        regs.x[22] = self.frame.preserved.x22 as u64;
        regs.x[23] = self.frame.preserved.x23 as u64;
        regs.x[24] = self.frame.preserved.x24 as u64;
        regs.x[25] = self.frame.preserved.x25 as u64;
        regs.x[26] = self.frame.preserved.x26 as u64;
        regs.x[27] = self.frame.preserved.x27 as u64;
        regs.x[28] = self.frame.preserved.x28 as u64;
        regs.x[29] = self.frame.preserved.x29 as u64;
        regs.x[30] = self.frame.preserved.x30 as u64;

        Ok(())
    }

    fn write_registers(
        &mut self,
        regs: &<Self::Arch as Arch>::Registers,
    ) -> TargetResult<(), Self> {
        self.frame.set_instr_pointer(regs.pc as usize);
        self.frame.set_stack_pointer(regs.sp as usize);

        self.frame.scratch.x0 = regs.x[0] as usize;
        self.frame.scratch.x1 = regs.x[1] as usize;
        self.frame.scratch.x2 = regs.x[2] as usize;
        self.frame.scratch.x3 = regs.x[3] as usize;
        self.frame.scratch.x4 = regs.x[4] as usize;
        self.frame.scratch.x5 = regs.x[5] as usize;
        self.frame.scratch.x6 = regs.x[6] as usize;
        self.frame.scratch.x7 = regs.x[7] as usize;
        self.frame.scratch.x8 = regs.x[8] as usize;
        self.frame.scratch.x9 = regs.x[9] as usize;
        self.frame.scratch.x10 = regs.x[10] as usize;
        self.frame.scratch.x11 = regs.x[11] as usize;
        self.frame.scratch.x12 = regs.x[12] as usize;
        self.frame.scratch.x13 = regs.x[13] as usize;
        self.frame.scratch.x14 = regs.x[14] as usize;
        self.frame.scratch.x15 = regs.x[15] as usize;
        self.frame.scratch.x16 = regs.x[16] as usize;
        self.frame.scratch.x17 = regs.x[17] as usize;
        self.frame.scratch.x18 = regs.x[18] as usize;
        self.frame.preserved.x19 = regs.x[19] as usize;
        self.frame.preserved.x20 = regs.x[20] as usize;
        self.frame.preserved.x21 = regs.x[21] as usize;
        self.frame.preserved.x22 = regs.x[22] as usize;
        self.frame.preserved.x23 = regs.x[23] as usize;
        self.frame.preserved.x24 = regs.x[24] as usize;
        self.frame.preserved.x25 = regs.x[25] as usize;
        self.frame.preserved.x26 = regs.x[26] as usize;
        self.frame.preserved.x27 = regs.x[27] as usize;
        self.frame.preserved.x28 = regs.x[28] as usize;
        self.frame.preserved.x29 = regs.x[29] as usize;
        self.frame.preserved.x30 = regs.x[30] as usize;

        Ok(())
    }

    fn read_addrs(
        &mut self,
        start_addr: <Self::Arch as Arch>::Usize,
        data: &mut [u8],
    ) -> TargetResult<usize, Self> {
        let slc = unsafe { core::slice::from_raw_parts(start_addr as *const u8, data.len()) };
        data.copy_from_slice(slc);
        Ok(data.len())
    }

    fn write_addrs(
        &mut self,
        start_addr: <Self::Arch as Arch>::Usize,
        data: &[u8],
    ) -> TargetResult<(), Self> {
        let slc = unsafe { core::slice::from_raw_parts_mut(start_addr as *mut u8, data.len()) };
        slc.copy_from_slice(data);
        Ok(())
    }

    fn support_resume(&mut self) -> Option<SingleThreadResumeOps<'_, Self>> {
        Some(self)
    }

    fn support_single_register_access(&mut self) -> Option<SingleRegisterAccessOps<'_, (), Self>> {
        None
    }
}

impl SingleThreadResume for InterruptTarget<'_> {
    fn resume(&mut self, _signal: Option<gdbstub::common::Signal>) -> Result<(), Self::Error> {
        unsafe {
            asm!(pop_special!(), pop_scratch!(), pop_preserved!(), "eret\n");
        }

        Ok(())
    }
}

pub fn enter_gdb_stub(frame: &mut InterruptFrame) {
    if GDB_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        log::error!("Cannot re-enter GDB stub");
        return;
    }

    let mut buffer = [0u8; 4096];

    let stub = match GdbStub::builder(Serial)
        .with_packet_buffer(&mut buffer)
        .build()
    {
        Ok(stub) => stub,
        Err(e) => {
            GDB_ACTIVE.store(false, Ordering::SeqCst);
            log::error!("Error initializing GDB stub: {e}");

            return;
        }
    };

    let mut target = InterruptTarget {
        frame,
        breakpoints: Default::default(),
    };

    let mut gdb = match stub.run_state_machine(&mut target) {
        Ok(gdb) => gdb,
        Err(e) => {
            GDB_ACTIVE.store(false, Ordering::SeqCst);
            log::error!("Error running GDB stub: {e:?}");
            return;
        }
    };

    let res = loop {
        gdb = match gdb {
            GdbStubStateMachine::Idle(mut gdb) => {
                let byte = gdb.borrow_conn().read();
                if let Ok(byte) = byte {
                    match gdb.incoming_data(&mut target, byte) {
                        Ok(gdb) => gdb,
                        Err(e) => break Err(e),
                    }
                } else {
                    unreachable!()
                }
            }
            GdbStubStateMachine::Running(gdb) => {
                match gdb.report_stop(&mut target, SingleThreadStopReason::DoneStep) {
                    Ok(gdb) => gdb,
                    Err(e) => break Err(e),
                }
            }
            GdbStubStateMachine::CtrlCInterrupt(gdb) => {
                match gdb.interrupt_handled(&mut target, None::<SingleThreadStopReason<u64>>) {
                    Ok(gdb) => gdb,
                    Err(e) => break Err(e),
                }
            }
            GdbStubStateMachine::Disconnected(gdb) => break Ok(gdb.get_reason()),
        }
    };

    GDB_ACTIVE.store(false, Ordering::SeqCst);

    log::debug!("Exiting GDB stub with {res:?}");
}
