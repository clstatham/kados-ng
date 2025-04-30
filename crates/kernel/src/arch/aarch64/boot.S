.section .text._start
.global _start
_start:
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

    // we're in El2, so we call go_to_el1 to switch to EL1
    ldr x0, =__stack_top
    b go_to_el1

    // go_to_el1 somehow returned, so hang
    b .hang
.in_el1:
    // we're in EL1, so we can just call kernel_main
    ldr x0, =__stack_top
    bl kernel_main
    // kernel_main somehow returned, so hang
.hang:
    wfe
    // hang forever
    b .hang
