use spin::Once;
use thiserror::Error;
use units::VirtAddr;

pub mod heap;
pub mod units;

pub static HDDM_PHYSICAL_OFFSET: Once<u64> = Once::new();

pub fn hhdm_physical_offset() -> u64 {
    *HDDM_PHYSICAL_OFFSET
        .get()
        .expect("expected HHDM_PHYSICAL_OFFSET to be initialized")
}

#[derive(Debug, Error)]
pub enum MmuError {
    #[error("Non-canonical physical address")]
    NonCanonicalPhysAddr(u64),
    #[error("Non-canonical virtual address")]
    NonCanonicalVirtAddr(u64),
    #[error("Null virtual address")]
    NullVirtAddr,
    #[error("Virtual address {0} is not aligned to {1}")]
    UnalignedVirtAddr(VirtAddr, u64),
}
