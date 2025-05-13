.section ".text.boot"
.global _start
_start:
    mov x20, x0
    mov x21, x1
    mov x22, x2
    mov x23, x3

    ldr x1, =_stack_top
    mov sp, x1

    mrs x1, mpidr_el1
    and x1, x1, #3
    cbnz x1, hang

    ldr x1, =LOAD_ADDR
    mov x0, x1
    ldr x2, =_start
    mov sp, x2
    ldr w3, =__loader_size
1:
    ldr x4, [x1], #8
    str x4, [x2], #8
    sub w3, w3, #1
    cbnz w3, 1b

    bl recv-0x60000
hang:
    wfe
    b hang

