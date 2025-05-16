use alloc::boxed::Box;
use fdt::{Fdt, node::FdtNode};
use spin::Once;

use crate::{
    arch::{Arch, ArchTrait},
    fdt::Phandle,
};

pub static mut IRQ_CHIP: Once<IrqChipDescriptor> = Once::new();

pub fn init(fdt: &Fdt) {
    #[allow(static_mut_refs)]
    unsafe {
        IRQ_CHIP.call_once(|| IrqChipDescriptor::new(fdt))
    };
}

pub unsafe fn irq_chip<'a>() -> &'a mut IrqChipDescriptor {
    #[allow(static_mut_refs)]
    unsafe {
        IRQ_CHIP.get_mut().unwrap()
    }
}

pub unsafe fn register_irq(irq: Irq, handler: impl IrqHandler) {
    if irq.as_usize() >= 1024 {
        log::error!("irq {} >= 1024", irq);
    }

    let irq_chip = unsafe { irq_chip() };
    if irq_chip.descs[irq.as_usize()].handler.is_some() {
        log::error!("irq {} already registered", irq);
        return;
    }

    irq_chip.descs[irq.as_usize()].handler = Some(Box::new(handler));
    irq_chip.enable_irq(irq);
    irq_chip.descs[irq.as_usize()]
        .handler
        .as_mut()
        .unwrap()
        .post_register_hook(irq);
}

pub unsafe fn enable_irq(irq: Irq) {
    unsafe { irq_chip().enable_irq(irq) }
}

int_wrapper!(pub Irq: u32);

#[derive(Debug, Clone, Copy)]
pub enum IrqCell {
    L1(u32),
    L2(u32, u32),
    L3(u32, u32, u32),
}

pub trait IrqHandler: Send + Sync + 'static {
    #[allow(unused)]
    fn post_register_hook(&mut self, irq: Irq) {}
    fn handle_irq(&mut self, irq: Irq);
}

pub trait IrqChip: IrqHandler {
    fn init(&mut self, fdt: &Fdt, descs: &mut [IrqHandlerDescriptor]);
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
impl IrqHandler for Null {
    fn handle_irq(&mut self, irq: Irq) {}
}

#[allow(unused)]
impl IrqChip for Null {
    fn init(&mut self, fdt: &Fdt, descs: &mut [IrqHandlerDescriptor]) {}
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
pub struct IrqHandlerDescriptor {
    pub index: usize,
    pub chip_irq: Irq,
    pub handler: Option<Box<dyn IrqHandler>>,
    pub used: bool,
}

impl IrqHandlerDescriptor {
    pub const INIT: Self = Self {
        index: 0,
        chip_irq: Irq(0),
        handler: None,
        used: false,
    };
}

pub struct IrqChipDescriptor {
    pub phandle: Phandle,
    pub chip: Box<dyn IrqChip>,
    pub descs: Box<[IrqHandlerDescriptor; 1024]>,
}

impl IrqChipDescriptor {
    pub fn new(fdt: &Fdt) -> Self {
        let mut this = Self {
            phandle: Phandle::default(),
            descs: Box::new([IrqHandlerDescriptor::INIT; 1024]),
            chip: Box::new(Null),
        };

        for node in fdt.all_nodes() {
            if node.property("interrupt-controller").is_some() {
                let compatible = node.compatible().unwrap().first();

                let Some(chip) = Arch::new_irq_chip(compatible) else {
                    continue;
                };

                this.phandle =
                    Phandle::from(node.property("phandle").unwrap().as_usize().unwrap() as u32);
                let intr_cells = node.interrupt_cells().unwrap();

                log::debug!(
                    "{}, compatible = {:?}, intr_cells = {:#x}, phandle = {:#x}",
                    node.name,
                    compatible,
                    intr_cells,
                    this.phandle.value()
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
