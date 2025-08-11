//! A lot of this code was taken from and inspired by Redox

use alloc::vec::Vec;
use fdt::standard_nodes::MemoryRegion;
pub use fdt::*;

use crate::mem::units::PhysAddr;

/// Initializes the FDT subsystem.
pub fn init(_fdt: &Fdt) {
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
    // dump(fdt);
}

/// Dumps the FDT structure to the log.
pub fn dump(fdt: &Fdt) {
    log::debug!("BEGIN FDT DUMP");

    log::debug!("    ROOT: {}", fdt.root().compatible().first());
    if let Some(aliases) = fdt.aliases() {
        log::debug!("    BEGIN ALIASES");
        for alias in aliases.all() {
            log::debug!("        {}: {}", alias.0, alias.1);
        }
        log::debug!("    END ALIASES");
    }
    log::debug!("    BEGIN NODES");
    for node in fdt.all_nodes() {
        log::debug!(
            "        {}: {:?}",
            node.name,
            node.compatible()
                .map(|c| c.all().collect::<Vec<_>>())
                .unwrap_or_default(),
        );
    }
    log::debug!("    END NODES");

    log::debug!("END FDT DUMP");
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Phandle(u32);

impl Phandle {
    /// Creates a new `Phandle` from a raw value.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw value of the `Phandle`.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Returns the MMIO address for a given memory region in the device tree.
#[must_use]
pub fn get_mmio_addr(fdt: &Fdt, region: &MemoryRegion) -> Option<PhysAddr> {
    let mut mapped_addr = region.starting_address as usize;
    let size = region.size.unwrap_or(0).saturating_sub(1);
    let last_addr = mapped_addr.saturating_add(size);

    if let Some(parent) = fdt.find_node("/soc") {
        let mut ranges = parent.ranges().map(Iterator::peekable)?;
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
