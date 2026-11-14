.section .text.demo,"ax"
.arch_extension simd
.global _start
_start:
    mov x1,#0x400000
    add x1,x1,#1,lsl #12
    ldr x4,[x1,#8]
    mrs x5,fpcr
    cbnz x5,bad
    mrs x5,fpsr
    cbnz x5,bad
    .irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
        umov x5,v\n\().d[0]
        cbnz x5,bad
        umov x5,v\n\().d[1]
        cbnz x5,bad
    .endr
    fmov d0,#1.5
    fadd d0,d0,d0
    fcvtzs x5,d0
    cmp x5,#3
    b.ne bad
    fmov s0,#1.5
    fadd s0,s0,s0
    fcvtzs x5,s0
    cmp x5,#3
    b.ne bad
    // IEEE 754 : conservation du signe de zéro et comparaison non ordonnée.
    mov w5,#0x80000000
    fmov s0,w5
    fadd s0,s0,s0
    fmov w9,s0
    cmp w9,w5
    b.ne bad
    mov x5,#0x7ff8000000000000
    fmov d0,x5
    fcmp d0,d0
    b.vc bad
    dup v0.2d,x4
    add v0.2d,v0.2d,v0.2d
    umov x5,v0.d[1]
    cmp x5,x4,lsl #1
    b.ne bad
    and x6,x4,#3
    lsl x7,x6,#22
    msr fpcr,x7
    msr fpsr,x6
    .irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
        add x5,x4,#\n
        dup v\n\().2d,x5
    .endr
again:
    mrs x5,fpcr
    cmp x5,x7
    b.ne bad
    mrs x5,fpsr
    cmp x5,x6
    b.ne bad
    .irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
        add x5,x4,#\n
        umov x9,v\n\().d[0]
        cmp x5,x9
        b.ne bad
        umov x9,v\n\().d[1]
        cmp x5,x9
        b.ne bad
    .endr
    ldr x2,[x1,#24]
    add x2,x2,#1
    str x2,[x1,#24]
    ldr x3,[x1,#32]
    cbz x3,again
    mov x0,x4
    b done
bad:
    mov x0,#255
done:
    mov x8,#0x10000
    add x8,x8,#4
    svc #0x4e
