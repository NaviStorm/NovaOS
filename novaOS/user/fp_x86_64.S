.section .text.demo,"ax"
.global _start
_start:
    mov rdi,0x401000
    mov r8,[rdi+8]
    sub rsp,1024
    and rsp,-64
    fxsave64 [rsp]
    cmp word ptr [rsp],0x037f
    jne bad
    cmp byte ptr [rsp+4],0
    jne bad
    cmp dword ptr [rsp+24],0x1f80
    jne bad
    lea rsi,[rsp+32]
    mov ecx,48
clear_check:
    cmp qword ptr [rsi],0
    jne bad
    add rsi,8
    loop clear_check
    mov eax,1
    cpuid
    xor r9d,r9d
    bt ecx,27
    jnc math
    xor ecx,ecx
    xgetbv
    and eax,4
    mov r9d,eax
    jz math
    mov eax,7
    xor edx,edx
    xsave64 [rsp]
    test byte ptr [rsp+512],4
    jz math
    lea rsi,[rsp+576]
    mov ecx,32
ymm_check:
    cmp qword ptr [rsi],0
    jne bad
    add rsi,8
    loop ymm_check
math:
    mov eax,0x3fc00000
    movd xmm0,eax
    addss xmm0,xmm0
    cvttss2si eax,xmm0
    cmp eax,3
    jne bad
    cvtsi2sd xmm0,r8
    addsd xmm0,xmm0
    cvttsd2si rax,xmm0
    lea rdx,[r8+r8]
    cmp rax,rdx
    jne bad
    // IEEE 754 : moins zéro plus moins zéro, NaN non ordonné.
    mov eax,0x80000000
    movd xmm0,eax
    addss xmm0,xmm0
    movd edx,xmm0
    cmp edx,eax
    jne bad
    mov rax,0x7ff8000000000001
    movq xmm0,rax
    ucomisd xmm0,xmm0
    jnp bad
    movd xmm0,r8d
    paddq xmm0,xmm0
    movq rax,xmm0
    lea rdx,[r8+r8]
    cmp rax,rdx
    jne bad
    test r9d,r9d
    jz control
    cvtsi2sd xmm0,r8
    vinsertf128 ymm0,ymm0,xmm0,1
    vaddpd ymm0,ymm0,ymm0
    vextractf128 xmm1,ymm0,1
    cvttsd2si rax,xmm1
    cmp rax,rdx
    jne bad
control:
    mov r11d,r8d
    and r11d,3
    shl r11d,10
    or r11d,0x037f
    mov [rsp+12],r11w
    fldcw [rsp+12]
    fild qword ptr [rdi+8]
    mov r10d,r8d
    and r10d,3
    shl r10d,13
    or r10d,0x1f80
    mov [rsp],r10d
    ldmxcsr [rsp]
    .irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15
        lea rax,[r8+\n]
        movq xmm\n,rax
        punpcklqdq xmm\n,xmm\n
        test r9d,r9d
        jz 2f
        vinsertf128 ymm\n,ymm\n,xmm\n,1
2:
    .endr
again:
    fnstcw [rsp+12]
    cmp [rsp+12],r11w
    jne bad
    stmxcsr [rsp]
    cmp [rsp],r10d
    jne bad
    fist dword ptr [rsp+8]
    cmp [rsp+8],r8d
    jne bad
    .irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15
        lea rax,[r8+\n]
        test r9d,r9d
        jz 3f
        vmovdqu [rsp+16],ymm\n
        cmp [rsp+32],rax
        jne bad
        cmp [rsp+40],rax
        jne bad
        jmp 4f
3:      movdqu [rsp+16],xmm\n
4:      cmp [rsp+16],rax
        jne bad
        cmp [rsp+24],rax
        jne bad
    .endr
    inc qword ptr [rdi+24]
    cmp qword ptr [rdi+32],0
    je again
    mov rdi,r8
    jmp done
bad:
    mov edi,255
done:
    mov eax,0x10004
    syscall
