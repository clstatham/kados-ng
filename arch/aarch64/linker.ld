OUTPUT_ARCH(aarch64)
ENTRY(kernel_main)

PHDRS
{
    text PT_LOAD;
    rodata PT_LOAD;
    data PT_LOAD;
    vectors PT_LOAD;
}


KERNEL_OFFSET = 0xffffffff80000000;

SECTIONS
{
    . = KERNEL_OFFSET;

    . = ALIGN(4096);

    .text : AT(ADDR(.text) - KERNEL_OFFSET) {
        __text_start = .;
        *(.text)
        . = ALIGN(0x800);
        KEEP(*(.text.vectors*))
	. = ALIGN(4096);
        __text_end = .;
    }

    .rodata : AT(ADDR(.rodata) - KERNEL_OFFSET) {
        __rodata_start = .;
        *(.rodata*)
	. = ALIGN(4096);
        __rodata_end = .;
    }

    .data : AT(ADDR(.data) - KERNEL_OFFSET) {
        __data_start = .;
        *(.data*)
	. = ALIGN(4096);
        __data_end = .;
        __bss_start = .;
        *(.bss*)
	. = ALIGN(4096);
        __bss_end = .;
    }


    /DISCARD/ : {
        *(.eh_frame*)
        *(.note .note.*)
    }
}
