use thiserror::Error;

use paging::table::PageTableEntry;
use units::{PhysAddr, VirtAddr};

pub mod heap;
pub mod paging;
pub mod units;

/// Error handling for memory operations.
#[derive(Debug, Error)]
pub enum MemError {
    #[error("Non-canonical physical address")]
    NonCanonicalPhysAddr(usize),
    #[error("Non-canonical virtual address")]
    NonCanonicalVirtAddr(usize),
    #[error("Null virtual address")]
    NullVirtAddr,
    #[error("Virtual address {0} is not aligned to {1}")]
    UnalignedVirtAddr(VirtAddr, usize),

    #[error("Page not present at {0}")]
    PageNotPresent(PhysAddr),
    #[error("Entry is huge page")]
    HugePage,
    #[error("Invalid page table index: {0}")]
    InvalidPageTableIndex(usize),
    #[error("Cannot go lower than page table level 0")]
    NoNextTable,
    #[error("Virtual address {0} is not a part of the page table at {1}")]
    NotPartOfTable(VirtAddr, PhysAddr),
    #[error("Page {0} is already mapped to {1:?}")]
    PageAlreadyMapped(VirtAddr, PageTableEntry),

    #[error("Out of physical memory")]
    OutOfMemory,
}
