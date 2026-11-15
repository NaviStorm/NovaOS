// C : seed, compteur privé, signal de fin. MXCSR est non volatil dans l'ABI C.
.globl fp_context
fp_context:
    sub rsp, 16
    stmxcsr [rsp]
    mov eax, [rsp]
    and eax, -24577
    mov r8d, edi
    and r8d, 3
    shl r8d, 13
    or r8d, eax
    mov [rsp+4], r8d
    ldmxcsr [rsp+4]

    cvtsi2sd xmm2, rdi
    addsd xmm2, xmm2
    cvttsd2si rax, xmm2
    lea rcx, [rdi+rdi]
    cmp rax, rcx
    jne 3f
    movq xmm0, rdi
    punpcklqdq xmm0, xmm0
    mov r9, rdi
2:
    stmxcsr [rsp+8]
    cmp [rsp+8], r8d
    jne 3f
    movhlps xmm1, xmm0
    movq rax, xmm1
    cmp rax, r9
    jne 3f
    movq rax, xmm0
    cmp rax, rdi
    jne 3f
    inc qword ptr [rsi]
    cmp qword ptr [rdx], 0
    je 2b
    jmp 4f
3:
    xor eax, eax
4:
    ldmxcsr [rsp]
    add rsp, 16
    ret
