// Sondes ABI SysV. Aucun appel, aucune instruction de cession. Chaque attente
// exige un changement du compteur exclusivement incrémenté par l’IRQ du timer.
.macro probe name, seed
.global \name
.hidden \name
\name:
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    sub rsp,16
    mov [rsp],rsp
    mov rax,[rip+NOVA_TICKS]
    neg rax
    mov [rsp+8],rax
    .set tag,\seed
    .irp reg,rdx,rbx,rsi,rdi,rbp,r8,r9,r10,r11,r12,r13,r14,r15
        mov \reg,tag
        .set tag,tag+1
    .endr
    std
    stc
1:  mov rcx,[rip+NOVA_TICKS]
    lea rcx,[rax+rcx]
    jrcxz 1b
    pushfq
    pop rcx
    cld
    and ecx,0x401
    cmp ecx,0x401
    jne 3f
    cmp rax,[rsp+8]
    jne 3f
    cmp rsp,[rsp]
    jne 3f
    .set tag,\seed
    .irp reg,rdx,rbx,rsi,rdi,rbp,r8,r9,r10,r11,r12,r13,r14,r15
        cmp \reg,tag
        jne 3f
        .set tag,tag+1
    .endr
    mov rcx,\seed
    mov rdx,[rip+NOVA_TICKS]
2:  cmp rdx,[rip+NOVA_TICKS]
    je 2b
    cmp rcx,\seed
    jne 3f
    mov eax,1
    jmp 4f
3:  xor eax,eax
4:  add rsp,16
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    ret
.endm
.pushsection .text.register_probe,"ax"
probe nova_probe_a, 0x1200
probe nova_probe_b, -0x2300
.popsection
