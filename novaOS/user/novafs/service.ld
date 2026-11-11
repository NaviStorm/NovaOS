ENTRY(_start)
PHDRS {
  text PT_LOAD FLAGS(5);
  ro PT_LOAD FLAGS(4);
  rw PT_LOAD FLAGS(6);
  stack PT_GNU_STACK FLAGS(6);
}
SECTIONS {
  . = 0x1000000;
  .text : { KEEP(*(.text.demo)) *(.text .text.*) } :text
  . = ALIGN(4096);
  .rodata : { *(.rodata .rodata.* .data.rel.ro .data.rel.ro.* .got .got.*) } :ro
  . = ALIGN(4096);
  .data : { *(.data .data.*) } :rw
  .bss (NOLOAD) : { *(.bss .bss.* COMMON) } :rw
  .stack_reserve (NOLOAD) : { . += 256K; } :stack
  /DISCARD/ : { *(.eh_frame* .note* .comment) }
}
