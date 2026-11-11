.pushsection .bss.syscall,"aw",@nobits
.balign 8
user_rsp: .skip 8
.global nova_fast,nova_iret
nova_fast: .skip 8
nova_iret: .skip 8
.popsection
.pushsection .text.syscall,"ax"
.global nova_syscall
.hidden nova_syscall
nova_syscall:
    mov [rip+user_rsp],rsp
    mov rsp,[rip+{tss}+4]
    push 0x4b
    push qword ptr [rip+user_rsp]
    push r11
    push 0x53
    push rcx
    .irp r,r15,r14,r13,r12,r11,r10,r9,r8,rbp,rdi,rsi,rdx,rcx,rbx,rax
        push \r
    .endr
    .irp seg,ds,es,fs,gs
        mov ax,\seg
        push rax
    .endr
    lea rdi,[rsp+32]
    cld
    call {dispatch}
    .irp seg,gs,fs,es,ds
        pop rax
        mov \seg,ax
    .endr
    .irp r,rax,rbx,rcx,rdx,rsi,rdi,rbp,r8,r9,r10,r11,r12,r13,r14,r15
        pop \r
    .endr
    // Le cadre a été validé. Le chemin complet respecte notamment TF et DF.
    test qword ptr [rsp+16],0x500
    jnz 1f
    inc qword ptr [rip+nova_fast]
    mov rsp,[rsp+24]
    sysretq
1:  inc qword ptr [rip+nova_iret]
    iretq
.popsection
