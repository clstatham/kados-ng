OUTPUT_ARCH(aarch64)
OUTPUT_FORMAT(elf64-littleaarch64)
ENTRY(_start)

BOOT_OFFSET = 0x80000;
KERNEL_OFFSET = 0xffffffff80000000;

SECTIONS
{
    . = BOOT_OFFSET;
    __boot_start = .;
    .boot ALIGN(4K) : SUBALIGN(0) { 
        KEEP(*(.boot))
    . = ALIGN(4K);
        __boot_stack_bottom = .;
    . = ALIGN(16K);
        __boot_stack_top = .;
        __boot_table = .;
        . += 256K;
        __boot_table_end = .;
    }
    __boot_end = .;

    PROVIDE(__kernel_phys_start = ALIGN(__boot_end, 4K));

    . = KERNEL_OFFSET;
    __kernel_virt_start = .;

    .text ALIGN(4K) : AT(__kernel_phys_start) {
        __text_start = .;
        *(.text*)
    . = ALIGN(0x800);
        KEEP(*(.text.vectors*))
	. = ALIGN(4096);
        __text_end = .;
    }

    .rodata ALIGN(4K) : AT(__kernel_phys_start + SIZEOF(.text)) {
        __rodata_start = .;
        *(.rodata*)
	. = ALIGN(4096);
        __rodata_end = .;
    }

    .data ALIGN(4K) : AT(__kernel_phys_start + SIZEOF(.text) + SIZEOF(.rodata)) {
        __data_start = .;
        *(.data*)
	. = ALIGN(4096);
        __data_end = .;
    }

    .bss (NOLOAD) : ALIGN(4K) {
        __bss_start = .;
        *(.bss* COMMON)
	. = ALIGN(4096);
        __stack_bottom = .;
        . += 0x20000;
        __stack_top = .;
        __bss_end = .;
    }
    __kernel_virt_end = .;
    PROVIDE(__kernel_phys_end = __kernel_phys_start + SIZEOF(.text) + SIZEOF(.rodata) + SIZEOF(.data) + SIZEOF(.bss));

    /DISCARD/ : {
        *(.eh_frame*)
    }
}
