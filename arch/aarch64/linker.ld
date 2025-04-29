OUTPUT_ARCH(aarch64)
ENTRY(_start)

SECTIONS
{
    . = 0x40080000;

    text : {
        *(.text._start)
        *(.text*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss (NOLOAD) : {
        *(.bss .bss.*)
        *(COMMON)

        . = ALIGN(16);
        __stack_start = .;
        . = . + 0x4000;      /* 16 KiB stack */
        __stack_top = .;
    }
}
