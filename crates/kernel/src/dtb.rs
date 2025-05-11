//! A lot of this code was taken from and inspired by Redox

use alloc::boxed::Box;
use derive_more::{Deref, Display, From, Into};
use fdt::{Fdt, node::FdtNode, standard_nodes::MemoryRegion};
use spin::Once;

use crate::{
    arch::{Arch, ArchTrait},
    mem::units::PhysAddr,
};

pub static mut IRQ_CHIP: Once<IrqChip> = Once::new();

pub unsafe fn irq_chip<'a>() -> &'a mut IrqChip {
    #[allow(static_mut_refs)]
    unsafe {
        IRQ_CHIP.get_mut().unwrap()
    }
}

pub fn init(fdt: &Fdt) {
    // for node in fdt.all_nodes() {
    //     println!(
    //         "{}: {}",
    //         node.name,
    //         node.compatible().map(|c| c.first()).unwrap_or_default()
    //     );

    //     for prop in node.properties() {
    //         println!("    {}", prop.name);
    //     }
    // }

    #[allow(static_mut_refs)]
    unsafe {
        IRQ_CHIP.call_once(|| IrqChip::new(fdt))
    };
}

pub unsafe fn register_irq(irq: Irq, handler: impl IrqHandlerTrait) {
    if irq.as_usize() >= 1024 {
        log::error!("irq {} >= 1024", irq);
    }

    let irq_chip = unsafe { irq_chip() };
    if irq_chip.descs[irq.as_usize()].handler.is_some() {
        log::error!("irq {} already registered", irq);
        return;
    }

    irq_chip.descs[irq.as_usize()].handler = Some(Box::new(handler));
}

pub unsafe fn enable_irq(irq: Irq) {
    unsafe { irq_chip().enable_irq(irq) }
}

#[derive(Debug, Copy, Clone)]
pub enum IrqCell {
    L1(u32),
    L2(u32, u32),
    L3(u32, u32, u32),
}

macro_rules! u32_wrappers {
    ($($name:ident),* $(,)?) => {
        $(
            #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deref, Display, From, Into)]
            pub struct $name(pub u32);

            impl $name {
                pub const fn as_u32(self) -> u32 {
                    self.0
                }

                pub const fn as_usize(self) -> usize {
                    self.as_u32() as usize
                }
            }
        )*
    };
}
u32_wrappers!(Irq, Phandle);

pub trait IrqHandlerTrait: Send + Sync + 'static {
    fn handle_irq(&mut self, irq: Irq);
}

pub trait IrqChipTrait: IrqHandlerTrait {
    fn init(&mut self, fdt: &Fdt, descs: &mut [IrqDescriptor]);
    fn ack(&mut self) -> Irq;
    fn eoi(&mut self, irq: Irq);
    fn translate_irq(&self, irq_data: IrqCell) -> Option<Irq>;
    fn enable_irq(&mut self, irq: Irq);
    fn disable_irq(&mut self, irq: Irq);
    fn manual_irq(&mut self, irq: Irq);
    fn is_irq_pending(&self, irq: Irq) -> bool;
}

pub struct Null;

#[allow(unused)]
impl IrqHandlerTrait for Null {
    fn handle_irq(&mut self, irq: Irq) {}
}

#[allow(unused)]
impl IrqChipTrait for Null {
    fn init(&mut self, fdt: &Fdt, descs: &mut [IrqDescriptor]) {}
    fn ack(&mut self) -> Irq {
        Irq(0)
    }
    fn eoi(&mut self, irq: Irq) {}
    fn translate_irq(&self, irq_data: IrqCell) -> Option<Irq> {
        None
    }
    fn disable_irq(&mut self, irq: Irq) {}
    fn enable_irq(&mut self, irq: Irq) {}
    fn manual_irq(&mut self, sgi: Irq) {}
    fn is_irq_pending(&self, irq: Irq) -> bool {
        false
    }
}

#[derive(Default)]
pub struct IrqDescriptor {
    pub index: usize,
    pub chip_irq: Irq,
    pub handler: Option<Box<dyn IrqHandlerTrait>>,
    pub used: bool,
}

impl IrqDescriptor {
    pub const INIT: Self = Self {
        index: 0,
        chip_irq: Irq(0),
        handler: None,
        used: false,
    };
}

pub struct IrqChip {
    pub phandle: Phandle,
    pub chip: Box<dyn IrqChipTrait>,
    pub descs: Box<[IrqDescriptor; 1024]>,
}

impl IrqChip {
    pub fn new(fdt: &Fdt) -> Self {
        let mut this = Self {
            phandle: Phandle::default(),
            descs: Box::new([IrqDescriptor::INIT; 1024]),
            chip: Box::new(Null),
        };

        for node in fdt.all_nodes() {
            if node.property("interrupt-controller").is_some() {
                let compatible = node.compatible().unwrap().first();

                let Some(chip) = Arch::new_irq_chip(compatible) else {
                    continue;
                };

                this.phandle =
                    Phandle(node.property("phandle").unwrap().as_usize().unwrap() as u32);
                let intr_cells = node.interrupt_cells().unwrap();

                log::debug!(
                    "{}, compatible = {:?}, intr_cells = {:#x}, phandle = {:#x}",
                    node.name,
                    compatible,
                    intr_cells,
                    this.phandle.as_u32()
                );

                if node.interrupt_parent().is_some() {
                    log::warn!("Interrupt chip parents are NYI");
                }

                this.chip = chip;
                break;
            }
        }

        this.chip.init(fdt, &mut this.descs[..]);

        this
    }

    pub fn ack(&mut self) -> Irq {
        self.chip.ack()
    }

    pub fn eoi(&mut self, irq: Irq) {
        self.chip.eoi(irq);
    }

    pub fn handle_irq(&mut self, irq: Irq) {
        if irq.as_usize() < 1024 {
            if let Some(handler) = &mut self.descs[irq.as_usize()].handler {
                handler.handle_irq(irq);
            } else {
                log::warn!("No handler for irq {}", irq);
            }
        }
    }

    pub fn enable_irq(&mut self, irq: Irq) {
        self.chip.enable_irq(irq);
    }

    pub fn disable_irq(&mut self, irq: Irq) {
        self.chip.disable_irq(irq);
    }

    pub fn translate_irq(&self, irq_data: &[u32]) -> Option<Irq> {
        let irq_data = match irq_data.len() {
            1 => IrqCell::L1(irq_data[0]),
            2 => IrqCell::L2(irq_data[0], irq_data[1]),
            3 => IrqCell::L3(irq_data[0], irq_data[1], irq_data[2]),
            _ => return None,
        };
        self.chip.translate_irq(irq_data)
    }

    pub fn manual_irq(&mut self, irq: Irq) {
        self.chip.manual_irq(irq);
    }
}

pub fn interrupt_parent<'a>(
    fdt: &'a Fdt<'a>,
    node: &'a FdtNode<'a, 'a>,
) -> Option<FdtNode<'a, 'a>> {
    node.interrupt_parent()
        .or_else(|| fdt.find_node("/soc").and_then(|soc| soc.interrupt_parent()))
        .or_else(|| fdt.find_node("/").and_then(|root| root.interrupt_parent()))
}

pub fn get_interrupt(fdt: &Fdt, node: &FdtNode, idx: usize) -> Option<IrqCell> {
    let interrupts = node.property("interrupts").unwrap();
    let parent_intr_cells = interrupt_parent(fdt, node)
        .unwrap()
        .interrupt_cells()
        .unwrap();
    let mut intr = interrupts
        .value
        .array_chunks::<4>()
        .map(|f| u32::from_be_bytes(*f))
        .skip(parent_intr_cells * idx);
    match parent_intr_cells {
        1 if let Some(a) = intr.next() => Some(IrqCell::L1(a)),
        2 if let Ok([a, b]) = intr.next_chunk() => Some(IrqCell::L2(a, b)),
        3 if let Ok([a, b, c]) = intr.next_chunk() => Some(IrqCell::L3(a, b, c)),
        _ => None,
    }
}

pub fn get_mmio_addr(fdt: &Fdt, region: &MemoryRegion) -> Option<PhysAddr> {
    let mut mapped_addr = region.starting_address as usize;
    let size = region.size.unwrap_or(0).saturating_sub(1);
    let last_addr = mapped_addr.saturating_add(size);

    if let Some(parent) = fdt.find_node("/soc") {
        let mut ranges = parent.ranges().map(|f| f.peekable())?;
        if ranges.peek().is_some() {
            let parent_range = ranges.find(|x| {
                x.child_bus_address <= mapped_addr && last_addr - x.child_bus_address <= x.size
            })?;
            mapped_addr = parent_range
                .parent_bus_address
                .checked_add(mapped_addr - parent_range.child_bus_address)?;
            mapped_addr.checked_add(size)?;
        }
    }

    PhysAddr::new(mapped_addr).ok()
}
