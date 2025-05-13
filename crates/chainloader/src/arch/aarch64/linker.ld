OUTPUT_ARCH(aarch64)
ENTRY(_start)

BOOT_ADDR = 0x20000;
LOAD_ADDR = 0x80000;

SECTIONS
{
    . = BOOT_ADDR;
    PROVIDE(_code = .);
    .text : { KEEP(*(.text.boot)) *(.text .text.*) }
    .rodata : { *(.rodata .rodata.* ) }
    PROVIDE(_data = .);
    .data : { 
        _stack_bottom = .;
        . = ALIGN(4K);
        _stack_top = .;
        *(.data .data.*) 
    }
    _end = .;

   /DISCARD/ : { *(.comment) *(.gnu*) *(.note*) *(.eh_frame*) }
}
__loader_size = (_end - _code)>>3;
