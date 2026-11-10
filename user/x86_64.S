// Image plate. DATA contient mode, motif, cible, progression et autorisation de sortie.
.section .text.demo,"ax"
.global _start
_start:
    .irp reg,rax,rbx,rcx,rdx,rdi,rsi,rbp,r8,r9,r10,r11,r12,r13,r14,r15
        test \reg,\reg
        jnz bad
    .endr
    mov rdi,0x401000
    mov rsi,[rdi+16]
    mov rax,[rdi]
    cmp eax,9
    je unknown_entry
    cmp eax,10
    je invalid_stack
    cmp eax,1
    je read_bad
    cmp eax,2
    je write_bad
    cmp eax,8
    je write_bad
    cmp eax,3
    je exec_bad
    cmp eax,4
    je privileged
    cmp eax,5
    je quick
    cmp eax,6
    je fp
    mov rdx,[rdi+8]
    push rdx
    .irp reg,rbx,rcx,rbp,r8,r9,r10,r11,r12,r13,r14,r15
        mov \reg,rdx
    .endr
again:
    cmp rdx,[rdi+8]
    jne bad
    cmp rdx,[rsp]
    jne bad
    .irp reg,rbx,rcx,rbp,r8,r9,r10,r11,r12,r13,r14,r15
        cmp \reg,rdx
        jne bad
    .endr
    cmp qword ptr [rdi],7
    jne count
    cmp [0x600000],rdx
    jne bad
count:
    inc qword ptr [rdi+24]
    cmp qword ptr [rdi+32],0
    je again
quick:
    mov rax,[rdi+8]
    int 0x81
read_bad:
    mov rax,[rsi]
    jmp bad
write_bad:
    mov byte ptr [rsi],0
    jmp bad
exec_bad:
    jmp rsi
unknown_entry:
    syscall
    jmp bad
invalid_stack:
    mov rsp,rsi
1:  jmp 1b
privileged:
    cli
    jmp bad
fp:
    fld1
bad:
    mov eax,255
    int 0x81
    ud2
