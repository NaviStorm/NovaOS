.pushsection .text.exceptions,"ax"
.macro stub n
nova_fault_\n:
    .if (\n != 8) && (\n != 10) && (\n != 11) && (\n != 12) && (\n != 13) && (\n != 14) && (\n != 17) && (\n != 21)
        push 0
    .endif
    mov edi,\n
    jmp 1f
.endm
.irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    stub \n
.endr
1:  mov rsi,[rsp]
    mov rdx,[rsp+8]
    mov rcx,[rsp+16]
    cld
    and rsp,-16
    call {handler}
.popsection
.pushsection .rodata.exceptions,"a"
.balign 8
.global nova_exception_entries
nova_exception_entries:
.irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    .quad nova_fault_\n
.endr
.popsection
