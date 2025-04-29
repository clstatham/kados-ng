
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
    ldr x0, =stack_top
    mov sp, x0

    ldr x1, =ARCH_TIMER_COUNTER_FREQUENCY
    mrs x2, CNTFRQ_EL0
    cmp x2, xzr
    b.eq hang
    str w2, [x1]

    bl kernel_main
hang:
    wfi
    b hang
