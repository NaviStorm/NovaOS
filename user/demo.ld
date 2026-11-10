ENTRY(_start)
SECTIONS {
    . = 0x400000;
    .text : { KEEP(*(.text.demo)) *(.text .text.* .rodata .rodata.*) }
    .edge 0x400ffc : { KEEP(*(.user_edge)) }
    /DISCARD/ : { *(.data* .bss* .eh_frame* .note* .comment .got*) }
    ASSERT(SIZEOF(.text) <= 4092, "image utilisateur trop grande")
}
