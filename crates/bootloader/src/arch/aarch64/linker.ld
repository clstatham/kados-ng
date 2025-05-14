OUTPUT_ARCH(aarch64)
OUTPUT_FORMAT(elf64-littleaarch64)

SECTIONS
{
    . = 0;
    .text ALIGN(4K) : { 
        KEEP(*(.text .text.*))
    }
    .rodata ALIGN(4K) : {
        KEEP(*(.rodata .rodata.*))
    }
    .data ALIGN(4K) : {
        KEEP(*(.data .data.*))
    }
    .bss (NOLOAD) : {
        *(.bss .bss.* COMMON)
    }

    /DISCARD/ : {
        *(.eh_frame*)
        *(.comment*)
    }
}
