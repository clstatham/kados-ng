.section .boot, "ax"
.global _start
_start:
    mov x19, x0
    ldr x1, =__boot_stack_top
    mov sp, x1

    mrs x1, SCTLR_EL2
    bic x1, x1, #1                // clear M bit
    msr SCTLR_EL2, x1
    isb
    
    mrs x1, SCTLR_EL1
    bic x1, x1, #1
    msr SCTLR_EL1, x1
    isb

    mov x0, x19
    mov x1, xzr
    bl mmu_init_el2
1:
    wfe
    b 1b
