ENTRY(nova_start)

PHDRS {
    requests PT_LOAD FLAGS(6);
    text     PT_LOAD FLAGS(5);
    rodata   PT_LOAD FLAGS(4);
    data     PT_LOAD FLAGS(6);
}

SECTIONS {
    . = 0xffffffff80000000;
    __image_start = .;
    .limine_requests : {
        KEEP(*(.limine_requests_start))
        KEEP(*(.limine_requests))
        KEEP(*(.limine_requests_end))
    } :requests

    .text ALIGN(0x1000) : { __text_start = .; *(.text .text.*) } :text
    .rodata ALIGN(0x1000) : { __rodata_start = .; *(.rodata .rodata.*) } :rodata
    .data ALIGN(0x1000) : { __data_start = .; *(.data .data.* .got .got.*) } :data
    .bss : { *(.bss .bss.*) *(COMMON) } :data

    __image_end = ALIGN(0x1000);

    /DISCARD/ : { *(.eh_frame*) *(.note*) *(.comment) }
}
