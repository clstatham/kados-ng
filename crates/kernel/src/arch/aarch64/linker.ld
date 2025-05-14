OUTPUT_ARCH(aarch64)
OUTPUT_FORMAT(elf64-littleaarch64)

BOOT_OFFSET = 0x80000;
KERNEL_OFFSET = 0xffffffff80000000;

ENTRY(_start)

PHDRS
{
    boot PT_LOAD FLAGS(7);
    kernel PT_LOAD FLAGS(7);
}

SECTIONS
{
    . = BOOT_OFFSET;
    __boot_start = .;
    .boot ALIGN(4K) : AT(BOOT_OFFSET) {
        KEEP( *libbootloader.a:(.text .text.* .rodata .rodata.*) )
    } : boot
    .boot.data ALIGN(4K) : AT(BOOT_OFFSET + SIZEOF(.boot)) {
        KEEP( *libbootloader.a:(.data .data.*) )
    . = ALIGN(4K);
        __boot_stack_bottom = .;
    . = ALIGN(16K);
        __boot_stack_top = .;
    } : boot
    .boot.bss (NOLOAD) : ALIGN(4K) {
        KEEP( *libbootloader.a:(.bss .bss.* COMMON) )
        . = ALIGN(4K);
        __boot_table = .;
        . += 256K;
        __boot_table_end = .;
    } : boot
    __boot_end = .;

    PROVIDE(__kernel_phys_start = ALIGN(__boot_end, 4K));

    . = KERNEL_OFFSET;
    __kernel_virt_start = .;

    .text ALIGN(4K) : AT(__kernel_phys_start) {
        __text_start = .;
        *(EXCLUDE_FILE (libbootloader.a) .text*)
    . = ALIGN(0x800);
        KEEP(*(.text.vectors*))
	. = ALIGN(4096);
        __text_end = .;
    } : kernel

    .rodata ALIGN(4K) : AT(__kernel_phys_start + SIZEOF(.text)) {
        __rodata_start = .;
        *(EXCLUDE_FILE (libbootloader.a) .rodata*)
	. = ALIGN(4096);
        __rodata_end = .;
    } : kernel

    .data ALIGN(4K) : AT(__kernel_phys_start + SIZEOF(.text) + SIZEOF(.rodata)) {
        __data_start = .;
        *(EXCLUDE_FILE (libbootloader.a) .data*)
	. = ALIGN(4096);
        __data_end = .;
    } : kernel

    .bss (NOLOAD) : ALIGN(4K) {
        __bss_start = .;
        *(EXCLUDE_FILE (libbootloader.a) .bss* COMMON)
	. = ALIGN(4096);
        __stack_bottom = .;
    . += 0x20000;
        __stack_top = .;
        __bss_end = .;
    } : kernel
    __kernel_virt_end = .;
    PROVIDE(__kernel_phys_end = __kernel_phys_start + SIZEOF(.text) + SIZEOF(.rodata) + SIZEOF(.data) + SIZEOF(.bss));

    /DISCARD/ : {
        *(.eh_frame*)
    }
}
