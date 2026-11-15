// C : seed, compteur privé, signal de fin. Restaurer FPCR avant le retour.
.arch_extension simd
.globl fp_context
fp_context:
    mrs x6, fpcr
    bic x7, x6, #0xc00000
    and x8, x0, #3
    orr x7, x7, x8, lsl #22
    msr fpcr, x7
    isb

    scvtf d1, x0
    fadd d1, d1, d1
    fcvtzs x4, d1
    cmp x4, x0, lsl #1
    b.ne 3f
    dup v0.2d, x0
    mov x3, x0
2:
    mrs x4, fpcr
    cmp x4, x7
    b.ne 3f
    umov x4, v0.d[0]
    cmp x4, x0
    b.ne 3f
    umov x4, v0.d[1]
    cmp x4, x3
    b.ne 3f
    ldr x4, [x1]
    add x4, x4, #1
    stlr x4, [x1]
    ldar x4, [x2]
    cbz x4, 2b
    b 4f
3:
    mov x0, #0
4:
    msr fpcr, x6
    isb
    ret
