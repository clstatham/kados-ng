use aarch64_cpu::registers::*;

pub unsafe fn init() {
    MAIR_EL1.set((0x44 << 8) | 0xff); // NORMAL_UNCACHED_MEMORY, NORMAL_WRITEBACK_MEMORY
}
