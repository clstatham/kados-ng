.section .boot, "ax"
.global _start
_start:
    mov x19, x0
    ldr x1, =__boot_stack_top
    mov sp, x1

    msr daifset, #0b1111

    mrs x1, SCTLR_EL2
    bic x1, x1, #1
    msr SCTLR_EL2, x1
    isb
    
    mrs x1, SCTLR_EL1
    bic x1, x1, #1
    msr SCTLR_EL1, x1
    isb

    mov x0, x19
    mov x1, xzr
    bl boot_el2
1:
    wfe
    b 1b

.equ DR, 0xFE201000
.equ FR, 0xFE201018
boot_uart_putc:
    ldr w1, =FR
    and w1, w1, #0x20
    bne boot_uart_putc
    ldr x1, =DR
    str w0, [x1], #1
    ret