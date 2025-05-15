#![allow(static_mut_refs)]

use core::arch::asm;

use alloc::collections::btree_map::BTreeMap;
use arrayvec::ArrayVec;
use gdbstub::{
    arch::Arch,
    common::Signal,
    conn::{Connection, ConnectionExt},
    stub::{GdbStub, SingleThreadStopReason, state_machine::GdbStubStateMachine},
    target::{
        Target, TargetError, TargetResult,
        ext::{
            base::{
                BaseOps,
                single_register_access::SingleRegisterAccessOps,
                singlethread::{
                    SingleThreadBase, SingleThreadResume, SingleThreadResumeOps,
                    SingleThreadSingleStep, SingleThreadSingleStepOps,
                },
            },
            breakpoints::{
                Breakpoints, BreakpointsOps, HwBreakpoint, HwBreakpointOps, SwBreakpoint,
                SwBreakpointOps,
            },
        },
    },
};
use gdbstub_arch::aarch64::AArch64;
use spin::Mutex;

use crate::{
    arch::vectors::InterruptFrame, mem::units::canonicalize_virtaddr, syscall::errno::Errno,
};

use super::serial::lock_uart;

static DEBUG_INTR_FRAME: Mutex<Option<InterruptFrame>> = Mutex::new(None);
static DEBUG_STATE: Mutex<Option<DebugState>> = Mutex::new(None);

fn reinit_state() {
    if DEBUG_STATE.try_lock().is_none_or(|lock| lock.is_some()) {
        return;
    }

    let stub = match GdbStub::builder(SerialConnection::default()).build() {
        Ok(stub) => stub,
        Err(e) => {
            panic!("Error reinitializing GDB stub: {e}");
        }
    };
    let mut target = KadosTarget::default();
    let stm = match stub.run_state_machine(&mut target) {
        Ok(server) => server,
        Err(e) => {
            panic!("Error re-running GDB state machine: {e:?}");
        }
    };

    DEBUG_STATE.lock().replace(DebugState { target, stm });
}

pub struct DebugState<'a> {
    pub target: KadosTarget,
    pub stm: GdbStubStateMachine<'a, KadosTarget, SerialConnection>,
}

#[derive(Default)]
pub struct SerialConnection {
    peeked: Option<u8>,
}

impl Connection for SerialConnection {
    type Error = ();
    fn write(&mut self, byte: u8) -> Result<(), Self::Error> {
        lock_uart().putchar(byte);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ConnectionExt for SerialConnection {
    fn read(&mut self) -> Result<u8, Self::Error> {
        if let Some(peeked) = self.peeked.take() {
            Ok(peeked)
        } else {
            Ok(lock_uart().getchar())
        }
    }

    fn peek(&mut self) -> Result<Option<u8>, Self::Error> {
        if let Some(byte) = lock_uart().try_getchar() {
            self.peeked = Some(byte);
        }
        Ok(self.peeked)
    }
}

enum Resume {
    Step,
    Continue,
}

#[derive(Default)]
pub struct KadosTarget {
    hw_breakpoints: ArrayVec<u64, 6>,
    sw_breakpoints: BTreeMap<u64, u32>,
    resume: Option<Resume>,
}

impl Target for KadosTarget {
    type Arch = AArch64;
    type Error = &'static str;

    fn base_ops(&mut self) -> BaseOps<'_, Self::Arch, Self::Error> {
        BaseOps::SingleThread(self)
    }

    fn support_breakpoints(&mut self) -> Option<BreakpointsOps<'_, Self>> {
        Some(self)
    }
}

impl Breakpoints for KadosTarget {
    fn support_sw_breakpoint(&mut self) -> Option<SwBreakpointOps<'_, Self>> {
        Some(self)
    }

    fn support_hw_breakpoint(&mut self) -> Option<HwBreakpointOps<'_, Self>> {
        Some(self)
    }
}

impl SwBreakpoint for KadosTarget {
    fn add_sw_breakpoint(
        &mut self,
        addr: <Self::Arch as Arch>::Usize,
        _kind: <Self::Arch as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        if self.sw_breakpoints.contains_key(&addr) {
            return Ok(false);
        }
        unsafe {
            let old_opcode = (addr as *mut u32).read_volatile();
            self.sw_breakpoints.insert(addr, old_opcode);
            (addr as *mut u32).write_volatile(0xd4207d00);
        }
        Ok(true)
    }

    fn remove_sw_breakpoint(
        &mut self,
        addr: <Self::Arch as Arch>::Usize,
        _kind: <Self::Arch as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        let Some(old_opcode) = self.sw_breakpoints.remove(&addr) else {
            return Ok(false);
        };
        unsafe {
            (addr as *mut u32).write_volatile(old_opcode);
            asm!("ic ivau, {}", "dsb ish", "isb", in(reg) addr);
        }
        Ok(true)
    }
}

impl HwBreakpoint for KadosTarget {
    fn add_hw_breakpoint(
        &mut self,
        addr: <Self::Arch as Arch>::Usize,
        _kind: <Self::Arch as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        if self.hw_breakpoints.contains(&addr) || self.hw_breakpoints.is_full() {
            return Ok(false);
        }
        let idx = self.hw_breakpoints.len();
        self.hw_breakpoints.push(addr);
        macro_rules! add_hw_breakpoint {
            ($slot:literal) => {
                asm!(
                    "msr dbgbvr{slot}_el1, {1}",
                    "mov {0}, #1",
                    "orr {0}, {0}, #(0b1111 << 5)",
                    "msr dbgbcr{slot}_el1, {0}",
                    out(reg) _,
                    in(reg) addr,
                    slot = const $slot,
                )
            };
        }
        unsafe {
            match idx {
                0 => add_hw_breakpoint!(0),
                1 => add_hw_breakpoint!(1),
                2 => add_hw_breakpoint!(2),
                3 => add_hw_breakpoint!(3),
                4 => add_hw_breakpoint!(4),
                5 => add_hw_breakpoint!(5),
                _ => unreachable!(),
            }
        }

        Ok(true)
    }

    fn remove_hw_breakpoint(
        &mut self,
        addr: <Self::Arch as Arch>::Usize,
        _kind: <Self::Arch as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        let Some(idx) = self.hw_breakpoints.iter().position(|&x| x == addr) else {
            return Ok(false);
        };
        self.hw_breakpoints.remove(idx);
        macro_rules! remove_hw_breakpoint {
            ($slot:literal) => {
                asm!(
                    "msr dbgbvr{slot}_el1, xzr",
                    "msr dbgbcr{slot}_el1, xzr",
                    slot = const $slot,
                )
            };
        }
        unsafe {
            match idx {
                0 => remove_hw_breakpoint!(0),
                1 => remove_hw_breakpoint!(1),
                2 => remove_hw_breakpoint!(2),
                3 => remove_hw_breakpoint!(3),
                4 => remove_hw_breakpoint!(4),
                5 => remove_hw_breakpoint!(5),
                _ => unreachable!(),
            }
        }

        Ok(true)
    }
}

impl SingleThreadBase for KadosTarget {
    #[inline(always)]
    fn read_registers(
        &mut self,
        regs: &mut <Self::Arch as Arch>::Registers,
    ) -> TargetResult<(), Self> {
        let mut frame = DEBUG_INTR_FRAME.lock();
        let frame = match frame.as_mut() {
            Some(frame) => frame,
            None => return Ok(()),
        };
        regs.pc = frame.instr_pointer() as u64;
        regs.sp = frame.stack_pointer() as u64;
        regs.x[0] = frame.scratch.x0 as u64;
        regs.x[1] = frame.scratch.x1 as u64;
        regs.x[2] = frame.scratch.x2 as u64;
        regs.x[3] = frame.scratch.x3 as u64;
        regs.x[4] = frame.scratch.x4 as u64;
        regs.x[5] = frame.scratch.x5 as u64;
        regs.x[6] = frame.scratch.x6 as u64;
        regs.x[7] = frame.scratch.x7 as u64;
        regs.x[8] = frame.scratch.x8 as u64;
        regs.x[9] = frame.scratch.x9 as u64;
        regs.x[10] = frame.scratch.x10 as u64;
        regs.x[11] = frame.scratch.x11 as u64;
        regs.x[12] = frame.scratch.x12 as u64;
        regs.x[13] = frame.scratch.x13 as u64;
        regs.x[14] = frame.scratch.x14 as u64;
        regs.x[15] = frame.scratch.x15 as u64;
        regs.x[16] = frame.scratch.x16 as u64;
        regs.x[17] = frame.scratch.x17 as u64;
        regs.x[18] = frame.scratch.x18 as u64;
        regs.x[19] = frame.preserved.x19 as u64;
        regs.x[20] = frame.preserved.x20 as u64;
        regs.x[21] = frame.preserved.x21 as u64;
        regs.x[22] = frame.preserved.x22 as u64;
        regs.x[23] = frame.preserved.x23 as u64;
        regs.x[24] = frame.preserved.x24 as u64;
        regs.x[25] = frame.preserved.x25 as u64;
        regs.x[26] = frame.preserved.x26 as u64;
        regs.x[27] = frame.preserved.x27 as u64;
        regs.x[28] = frame.preserved.x28 as u64;
        regs.x[29] = frame.preserved.x29 as u64;
        regs.x[30] = frame.preserved.x30 as u64;

        Ok(())
    }

    fn write_registers(
        &mut self,
        regs: &<Self::Arch as Arch>::Registers,
    ) -> TargetResult<(), Self> {
        let mut frame = DEBUG_INTR_FRAME.lock();
        let Some(frame) = frame.as_mut() else {
            return Ok(());
        };
        frame.set_instr_pointer(regs.pc as usize);
        frame.set_stack_pointer(regs.sp as usize);

        frame.scratch.x0 = regs.x[0] as usize;
        frame.scratch.x1 = regs.x[1] as usize;
        frame.scratch.x2 = regs.x[2] as usize;
        frame.scratch.x3 = regs.x[3] as usize;
        frame.scratch.x4 = regs.x[4] as usize;
        frame.scratch.x5 = regs.x[5] as usize;
        frame.scratch.x6 = regs.x[6] as usize;
        frame.scratch.x7 = regs.x[7] as usize;
        frame.scratch.x8 = regs.x[8] as usize;
        frame.scratch.x9 = regs.x[9] as usize;
        frame.scratch.x10 = regs.x[10] as usize;
        frame.scratch.x11 = regs.x[11] as usize;
        frame.scratch.x12 = regs.x[12] as usize;
        frame.scratch.x13 = regs.x[13] as usize;
        frame.scratch.x14 = regs.x[14] as usize;
        frame.scratch.x15 = regs.x[15] as usize;
        frame.scratch.x16 = regs.x[16] as usize;
        frame.scratch.x17 = regs.x[17] as usize;
        frame.scratch.x18 = regs.x[18] as usize;
        frame.preserved.x19 = regs.x[19] as usize;
        frame.preserved.x20 = regs.x[20] as usize;
        frame.preserved.x21 = regs.x[21] as usize;
        frame.preserved.x22 = regs.x[22] as usize;
        frame.preserved.x23 = regs.x[23] as usize;
        frame.preserved.x24 = regs.x[24] as usize;
        frame.preserved.x25 = regs.x[25] as usize;
        frame.preserved.x26 = regs.x[26] as usize;
        frame.preserved.x27 = regs.x[27] as usize;
        frame.preserved.x28 = regs.x[28] as usize;
        frame.preserved.x29 = regs.x[29] as usize;
        frame.preserved.x30 = regs.x[30] as usize;

        Ok(())
    }

    fn read_addrs(
        &mut self,
        start_addr: <Self::Arch as Arch>::Usize,
        data: &mut [u8],
    ) -> TargetResult<usize, Self> {
        if canonicalize_virtaddr(start_addr as usize) != start_addr as usize {
            return Err(TargetError::Errno(Errno::EFAULT as u8));
        }
        let slc = unsafe { core::slice::from_raw_parts(start_addr as *const u8, data.len()) };
        data.copy_from_slice(slc);
        Ok(data.len())
    }

    fn write_addrs(
        &mut self,
        start_addr: <Self::Arch as Arch>::Usize,
        data: &[u8],
    ) -> TargetResult<(), Self> {
        if canonicalize_virtaddr(start_addr as usize) != start_addr as usize {
            return Err(TargetError::Errno(Errno::EFAULT as u8));
        }
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

impl SingleThreadResume for KadosTarget {
    fn resume(&mut self, _signal: Option<Signal>) -> Result<(), Self::Error> {
        self.resume = Some(Resume::Continue);
        Ok(())
    }

    fn support_single_step(&mut self) -> Option<SingleThreadSingleStepOps<'_, Self>> {
        None
    }
}

impl SingleThreadSingleStep for KadosTarget {
    fn step(&mut self, _signal: Option<Signal>) -> Result<(), Self::Error> {
        self.resume = Some(Resume::Step);
        Ok(())
    }
}

pub enum StopReason {
    SwBreakpoint,
    HwBreakpoint,
}

pub fn on_irq(frame: &mut InterruptFrame, reason: StopReason) {
    let Some(mut state) = DEBUG_STATE.try_lock() else {
        panic!("reentry into GDB stub");
    };

    let state = match state.take() {
        Some(state) => state,
        None => {
            drop(state);
            reinit_state();
            DEBUG_STATE.lock().take().unwrap()
        }
    };

    let DebugState {
        mut target,
        mut stm,
    } = state;

    *DEBUG_INTR_FRAME.lock() = Some(*frame);

    loop {
        stm = match stm {
            GdbStubStateMachine::Idle(mut stm) => {
                let conn = stm.borrow_conn();
                let byte = conn.read().unwrap();

                match stm.incoming_data(&mut target, byte) {
                    Ok(stm) => stm,
                    Err(e) if e.is_target_error() => {
                        log::error!("Debugger raised fatal error: {e:?}");
                        break;
                    }
                    Err(e) => {
                        log::error!("Internal GDBstub error: {e:?}");
                        break;
                    }
                }
            }
            GdbStubStateMachine::Running(mut stm) => {
                let conn = stm.borrow_conn();

                if conn.peek().unwrap().is_some() {
                    let byte = conn.read().unwrap();

                    match stm.incoming_data(&mut target, byte) {
                        Ok(stm) => stm,
                        Err(e) if e.is_target_error() => {
                            log::error!("Debugger raised fatal error: {e:?}");
                            break;
                        }
                        Err(e) => {
                            log::error!("Internal GDBstub error: {e:?}");
                            break;
                        }
                    }
                } else if let Some(resume) = target.resume.take() {
                    match resume {
                        Resume::Continue => {
                            log::debug!("resuming");
                            unsafe {
                                asm!(
                                    "mrs {0}, mdscr_el1",
                                    "bic {0}, {0}, #(1<<0)", // SS
                                    "msr mdscr_el1, {0}",
                                    out(reg) _,
                                );
                            };
                        }
                        Resume::Step => todo!("step"),
                    }

                    *DEBUG_STATE.lock() = Some(DebugState {
                        target,
                        stm: GdbStubStateMachine::Running(stm),
                    });
                    break;
                } else {
                    // must be stopped on a breakpoint
                    let reason = match reason {
                        StopReason::HwBreakpoint => SingleThreadStopReason::HwBreak(()),
                        StopReason::SwBreakpoint => SingleThreadStopReason::SwBreak(()),
                    };
                    match stm.report_stop(&mut target, reason) {
                        Ok(stm) => stm,
                        Err(e) if e.is_target_error() => {
                            log::error!("Debugger raised fatal error: {e:?}");
                            break;
                        }
                        Err(e) => {
                            log::error!("Internal GDBstub error: {e:?}");
                            break;
                        }
                    }
                }
            }
            GdbStubStateMachine::CtrlCInterrupt(stm) => {
                match stm
                    .interrupt_handled(&mut target, Some(SingleThreadStopReason::<u64>::DoneStep))
                {
                    Ok(stm) => stm,
                    Err(e) => {
                        log::error!("GDBstub error: {e:?}");
                        break;
                    }
                }
            }
            GdbStubStateMachine::Disconnected(_stm) => {
                log::error!("GDBstub disconnected");
                break;
            }
        };
    }

    *frame = DEBUG_INTR_FRAME.lock().take().unwrap();
}
