use core::ops::Range;

use fdt::Fdt;

use crate::{
    fdt::get_mmio_addr,
    irq::{Irq, IrqCell, IrqChip, IrqHandler, IrqHandlerDescriptor},
    mem::units::{PhysAddr, VirtAddr},
    syscall::errno::Errno,
};

use super::drivers::mmio::Mmio;

const GICD_CTLR: usize = 0x000;
const GICD_TYPER: usize = 0x004;
const GICD_ISENABLER: usize = 0x100;
const GICD_ISPENDR: usize = 0x200;
const GICD_ICENABLER: usize = 0x180;
const GICD_IPRIORITY: usize = 0x400;
const GICD_ITARGETSR: usize = 0x800;
const GICD_ICFGR: usize = 0xc00;

const GICC_EOIR: usize = 0x0010;
const GICC_IAR: usize = 0x000c;
const GICC_CTLR: usize = 0x0000;
const GICC_PMR: usize = 0x0004;

#[derive(Clone, Copy, Debug, Default)]
pub struct GicAddrs {
    pub dist_phys: PhysAddr,
    pub cpu_phys: PhysAddr,
}

#[derive(Default)]
pub struct Gic {
    pub dist: GicDist,
    pub cpu: GicCpu,
    pub irq_range: Range<usize>,
}

impl Gic {
    pub fn parse(fdt: &Fdt) -> Result<GicAddrs, Errno> {
        if let Some(node) = fdt.find_compatible(&["arm,gic-400"]) {
            let region_iter = node.reg().unwrap();
            let mut addrs = GicAddrs::default();
            let mut idx = 0;

            for region in region_iter {
                match region.size {
                    Some(0) => {
                        break;
                    }
                    None => break,
                    _ => {}
                };

                let addr = get_mmio_addr(fdt, &region).unwrap();
                match idx {
                    0 => addrs.dist_phys = addr,
                    2 => addrs.cpu_phys = addr,
                    _ => break,
                };
                idx += 2;
            }

            if idx == 4 {
                Ok(addrs)
            } else {
                Err(Errno::EINVAL)
            }
        } else {
            Err(Errno::EINVAL)
        }
    }
}

impl IrqHandler for Gic {
    fn handle_irq(&mut self, _irq: Irq) {
        log::warn!("handle_irq() called on Gic (no-op)");
    }
}

impl IrqChip for Gic {
    fn init(&mut self, fdt: &Fdt, descs: &mut [IrqHandlerDescriptor]) {
        let GicAddrs {
            dist_phys,
            cpu_phys,
        } = Gic::parse(fdt).unwrap();
        let dist_virt = dist_phys.as_hhdm_virt();
        let cpu_virt = cpu_phys.as_hhdm_virt();

        log::debug!("GIC_DIST @ {dist_virt}, GIC_CPU @ {cpu_virt}");

        unsafe {
            self.dist.init(dist_virt);
            self.cpu.init(cpu_virt);
        }

        let count = self.dist.num_irqs.min(1024) as usize;
        let mut i = 0;
        while i < count && i < 1024 {
            descs[i].chip_irq = Irq::from(i as u32);
            descs[i].used = true;
            i += 1;
        }
        self.irq_range = 0..count;
    }

    fn ack(&mut self) -> Irq {
        unsafe { self.cpu.ack_irq() }
    }

    fn eoi(&mut self, irq: Irq) {
        unsafe { self.cpu.eoi_irq(irq) }
    }

    fn enable_irq(&mut self, irq: Irq) {
        unsafe { self.dist.enable_irq(irq) }
    }

    fn disable_irq(&mut self, irq: Irq) {
        unsafe { self.dist.disable_irq(irq) }
    }

    fn translate_irq(&self, irq_data: IrqCell) -> Option<Irq> {
        let off = match irq_data {
            IrqCell::L3(0, irq, _flags) => irq as usize,
            IrqCell::L3(1, irq, _flags) => irq as usize,
            _ => return None,
        };
        Some(Irq::from((off + self.irq_range.start) as u32))
    }

    fn manual_irq(&mut self, irq: Irq) {
        unsafe { self.dist.manual_irq(irq) }
    }

    fn is_irq_pending(&self, irq: Irq) -> bool {
        unsafe { self.dist.is_irq_pending(irq) }
    }
}

#[derive(Debug, Default)]
pub struct GicDist {
    pub base: Mmio<u32>,
    pub num_irqs: u32,
}

impl GicDist {
    pub unsafe fn init(&mut self, addr: VirtAddr) {
        self.base.addr = addr;

        unsafe {
            self.base.write_assert(GICD_CTLR, 0);

            let typer = self.base.read(GICD_TYPER);
            let num_cpus = ((typer & (0x7 << 5)) >> 5) + 1;
            let num_irqs = ((typer & 0x1f) + 1) * 32;
            log::debug!("GIC_DIST supports {} CPUs and {} IRQs", num_cpus, num_irqs);
            self.num_irqs = num_irqs;

            // let bit = 1 << ((irq as u32 % 16) * 2 + 1);
            // self.base.write_assert(off, bit); // level-trigger

            // for irq in 0..num_irqs as usize {

            // }

            self.base.write_assert(GICD_CTLR, 1 << 0);
        }
    }

    pub unsafe fn enable_irq(&mut self, irq: Irq) {
        let irq = irq.as_usize();
        log::debug!("enabling IRQ {irq} in ISENABLER");
        if irq > 31 {
            let ext_off = GICD_ITARGETSR + ((irq / 4) * 4);
            let int_off = (irq % 4) * 8;
            unsafe { self.base.set(ext_off, 1 << int_off) }; // target cpu 0
        }

        let ext_off = GICD_IPRIORITY + ((irq / 4) * 4);
        let int_off = (irq % 4) * 8;
        unsafe { self.base.set(ext_off, 0xa0 << int_off) }; // priority

        let off = GICD_ICFGR + ((irq / 16) * 4);
        let bit = 0b11 << ((irq as u32 % 16) * 2);
        unsafe { self.base.clear(off, bit) }; // edge-trigger

        let off = GICD_ISENABLER + ((irq / 32) * 4);
        let bit = 1 << (irq % 32);
        unsafe {
            self.base.set_assert(off, bit); // enable
        }
    }

    pub unsafe fn is_irq_pending(&self, irq: Irq) -> bool {
        let off = GICD_ISPENDR + ((irq.as_usize() / 32) * 4);
        let bit = 1 << (irq.as_usize() % 32);
        unsafe { self.base.read(off) & bit == bit }
    }

    pub unsafe fn disable_irq(&mut self, irq: Irq) {
        log::debug!("disabling IRQ {irq} in ICENABLER");
        let off = GICD_ICENABLER + ((irq.as_usize() / 32) * 4);
        let bit = 1 << (irq.as_usize() % 32);
        unsafe {
            self.base.write_assert(off, bit);
        }
    }

    pub unsafe fn manual_irq(&mut self, irq: Irq) {
        log::debug!("manually triggering IRQ {irq} in ISPENDR");
        let off = GICD_ISPENDR + ((irq.as_usize() / 32) * 4);
        let bit = 1 << (irq.as_usize() % 32);
        unsafe {
            self.base.write_assert(off, bit);
        }
    }
}

#[derive(Debug, Default)]
pub struct GicCpu {
    pub base: Mmio<u32>,
}

impl GicCpu {
    pub unsafe fn init(&mut self, addr: VirtAddr) {
        self.base.addr = addr;

        unsafe {
            self.base.write_assert(GICC_CTLR, 0);
            self.base.write_assert(GICC_PMR, 0xf0);
            self.base.write_assert(GICC_CTLR, 1 << 0);
        }
    }

    pub unsafe fn ack_irq(&mut self) -> Irq {
        unsafe { Irq::from(self.base.read(GICC_IAR)) }
    }

    pub unsafe fn eoi_irq(&mut self, irq: Irq) {
        unsafe { self.base.write(GICC_EOIR, irq.value()) };
    }
}
