use alloc::boxed::Box;
use fdt::{Fdt, node::FdtNode};
use spin::Once;

use crate::{
    arch::{Arch, Architecture},
    fdt::Phandle,
    sync::{IrqMutex, IrqMutexGuard},
};

/// A static reference to the IRQ chip.
pub static IRQ_CHIP: Once<IrqMutex<IrqChipDescriptor>> = Once::new();

/// Initializes the IRQ chip with the given flattened device tree (FDT).
pub fn init(fdt: &Fdt) {
    #[allow(static_mut_refs)]
    IRQ_CHIP.call_once(|| IrqMutex::new(IrqChipDescriptor::new(fdt)));
}

/// Returns a mutex guard to the IRQ chip descriptor.
///
/// Note that this will disable interrupts while the guard is held.
/// The guard must not be held across a context switch or return
/// from an interrupt handler.
/// If you need to hold the guard across a context switch,
/// you can (unsafely) call `force_unlock()` on the guard
/// as the very last thing before returning.
pub fn irq_chip<'a>() -> IrqMutexGuard<'a, IrqChipDescriptor> {
    #[allow(static_mut_refs)]
    IRQ_CHIP.get().expect("IRQ chip not initialized").lock()
}

/// Registers an IRQ handler for the given IRQ.
pub unsafe fn register_irq(irq: Irq, handler: impl IrqHandler) {
    if irq.as_usize() >= 1024 {
        log::error!("irq {} >= 1024", irq);
    }

    let mut irq_chip = irq_chip();
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

    log::debug!("Registered IRQ handler for {}", irq);
}

/// Enables the given IRQ.
pub fn enable_irq(irq: Irq) {
    irq_chip().enable_irq(irq)
}

int_wrapper!(pub Irq: u32);

/// Represents the IRQ cell structure used in device trees.
#[derive(Debug, Clone, Copy)]
pub enum IrqCell {
    /// A single IRQ cell.
    L1(u32),
    /// Two IRQ cells.
    L2(u32, u32),
    /// Three IRQ cells.
    L3(u32, u32, u32),
}

/// Represents an IRQ handler that can be registered for a specific IRQ.
pub trait IrqHandler: Send + Sync + 'static {
    /// Called when the IRQ handler is registered.
    /// Can be left unimplemented if not needed.
    #[allow(unused)]
    fn post_register_hook(&mut self, irq: Irq) {}

    /// Handles the IRQ when it is triggered.
    fn handle_irq(&mut self, irq: Irq);
}

/// Represents an IRQ chip that can handle interrupts.
pub trait IrqChip: IrqHandler {
    /// Initializes the IRQ chip with the given FDT and IRQ handler descriptor array.
    ///
    /// This function is responsible for setting up the IRQ chip and its handlers.
    fn init(&mut self, fdt: &Fdt, descs: &mut [IrqHandlerDescriptor]);

    /// Acknowledges the IRQ and returns the IRQ number.
    fn ack(&mut self) -> Irq;

    /// Sends an end-of-interrupt (EOI) signal for the given IRQ.
    fn eoi(&mut self, irq: Irq);

    /// Translates the IRQ data from the device tree into an IRQ number.
    fn translate_irq(&self, irq_data: IrqCell) -> Option<Irq>;

    /// Enables the given IRQ.
    fn enable_irq(&mut self, irq: Irq);

    /// Disables the given IRQ.
    fn disable_irq(&mut self, irq: Irq);

    /// Manually triggers the given IRQ.
    /// This is typically used for software-generated interrupts (SGIs).
    fn manual_irq(&mut self, irq: Irq);

    /// Checks if the given IRQ is pending.
    fn is_irq_pending(&self, irq: Irq) -> bool;
}

/// A null IRQ handler that does nothing.
///
/// This is used as a default handler when no specific handler is registered.
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

/// A descriptor for an IRQ handler.
///
/// This structure contains information about the IRQ handler,
/// the IRQ number, and whether the handler is in use.
#[derive(Default)]
pub struct IrqHandlerDescriptor {
    /// The index of the IRQ handler in the descriptor array.
    pub index: usize,

    /// The IRQ number associated with this handler.
    pub chip_irq: Irq,

    /// The IRQ handler itself.
    pub handler: Option<Box<dyn IrqHandler>>,

    /// Indicates whether this handler is currently in use.
    pub used: bool,
}

impl IrqHandlerDescriptor {
    /// A constant representing an uninitialized IRQ handler descriptor.
    pub const INIT: Self = Self {
        index: 0,
        chip_irq: Irq(0),
        handler: None,
        used: false,
    };
}

/// A descriptor for an IRQ chip.
///
/// This structure contains the IRQ chip's phandle,
/// the IRQ chip itself, and an array of IRQ handler descriptors.
pub struct IrqChipDescriptor {
    /// The phandle of the IRQ chip in the device tree.
    pub phandle: Phandle,

    /// The IRQ chip itself.
    pub chip: Box<dyn IrqChip>,

    /// An array of IRQ handler descriptors.
    pub descs: Box<[IrqHandlerDescriptor; 1024]>,
}

impl IrqChipDescriptor {
    /// Creates a new `IrqChipDescriptor` instance from the given FDT.
    pub fn new(fdt: &Fdt) -> Self {
        let mut this = Self {
            phandle: Phandle::default(),
            descs: Box::new([IrqHandlerDescriptor::INIT; 1024]),
            chip: Box::new(Null),
        };

        // find the first interrupt controller node that is compatible with the architecture
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

    /// Acknowledges the IRQ and returns the IRQ number.
    pub fn ack(&mut self) -> Irq {
        self.chip.ack()
    }

    /// Sends an end-of-interrupt (EOI) signal for the given IRQ.
    pub fn eoi(&mut self, irq: Irq) {
        self.chip.eoi(irq);
    }

    /// Runs the IRQ handler for the given IRQ, if it has been registered.
    pub fn handle_irq(&mut self, irq: Irq) {
        if irq.as_usize() < 1024 {
            if let Some(handler) = &mut self.descs[irq.as_usize()].handler {
                handler.handle_irq(irq);
            } else {
                log::warn!("No handler for irq {}", irq);
            }
        }
    }

    /// Enables the given IRQ.
    pub fn enable_irq(&mut self, irq: Irq) {
        self.chip.enable_irq(irq);
    }

    /// Disables the given IRQ.
    pub fn disable_irq(&mut self, irq: Irq) {
        self.chip.disable_irq(irq);
    }

    /// Translates the IRQ data from the device tree into an IRQ number.
    pub fn translate_irq(&self, irq_data: &[u32]) -> Option<Irq> {
        let irq_data = match irq_data.len() {
            1 => IrqCell::L1(irq_data[0]),
            2 => IrqCell::L2(irq_data[0], irq_data[1]),
            3 => IrqCell::L3(irq_data[0], irq_data[1], irq_data[2]),
            _ => return None,
        };
        self.chip.translate_irq(irq_data)
    }

    /// Manually triggers the given IRQ.
    pub fn manual_irq(&mut self, irq: Irq) {
        self.chip.manual_irq(irq);
    }
}

/// Returns the parent interrupt node for the given FDT node.
fn interrupt_parent<'a>(fdt: &'a Fdt<'a>, node: &'a FdtNode<'a, 'a>) -> Option<FdtNode<'a, 'a>> {
    node.interrupt_parent()
        .or_else(|| fdt.find_node("/soc").and_then(|soc| soc.interrupt_parent()))
        .or_else(|| fdt.find_node("/").and_then(|root| root.interrupt_parent()))
}

/// Returns the interrupt cell for the given FDT node and index.
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
