.section .bss
.align 16
stack_bottom:
    .skip 4096 * 8
stack_top:

.section .bss
.align 12
.global early_page_table
early_page_table:
    .skip 4096

.section .text
.global _start
_start:
    // check to make sure we're the boot core
    mrs x0, MPIDR_EL1
    and x0, x0, #0b11
    cmp x0, #0
    b.ne .hang // if we're not the boot core, hang

    // set up the initial stack
    ldr x0, =stack_top
    mov sp, x0

    // set up the timer registers
    ldr x1, =ARCH_TIMER_COUNTER_FREQUENCY
    mrs x2, CNTFRQ_EL0
    cmp x2, xzr
    b.eq .hang // if CNTFRQ_EL0 is 0, hang
    str w2, [x1]

    // check the current exception level
    mrs x0, CurrentEL
    // check for EL1
    cmp x0, #0b0100
    b.eq .in_el1 // if we're already in EL1, go to kernel_main
    // check for EL2
    cmp x0, #0b1000
    b.ne .hang // if we're not in EL2, hang

    // we're  in El2, so we call arch_main to switch to EL1
    ldr x0, =stack_top
    b arch_main

    // arch_main somehow returned, so hang
    b .hang
.in_el1:
    // we're in EL1, so we can just call kernel_main
    bl kernel_main
.hang:
    wfe
    b .hang
