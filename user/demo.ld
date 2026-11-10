ENTRY(_start)
SECTIONS {
    . = 0x400000;
    .text : { KEEP(*(.text.demo)) }
    /DISCARD/ : { *(.text .text.* .rodata* .data* .bss* .eh_frame* .note* .comment .got*) }
    ASSERT(SIZEOF(.text) <= 4096, "image utilisateur trop grande")
}
