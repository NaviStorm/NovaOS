// Deux passages : x3..x30 et NZCV vivants au premier, x0..x2 au second.
// La boucle de lecture du compteur ne modifie pas NZCV.
.macro probe name, seed
.global \name
.hidden \name
\name:
    sub sp,sp,#128
    stp x18,x19,[sp]
    stp x20,x21,[sp,#16]
    stp x22,x23,[sp,#32]
    stp x24,x25,[sp,#48]
    stp x26,x27,[sp,#64]
    stp x28,x29,[sp,#80]
    str x30,[sp,#96]
    mov x0,sp
    str x0,[sp,#104]
    adrp x0,NOVA_TICKS
    add x0,x0,:lo12:NOVA_TICKS
    ldr x1,[x0]
    .irp n,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30
        mov x\n,#(\seed+\n)
    .endr
    cmp xzr,xzr
1:  ldr x2,[x0]
    eor x2,x2,x1
    cbz x2,1b
    mrs x2,nzcv
    mov x1,#0x60000000
    cmp x1,x2
    b.ne 3f
    ldr x2,[sp,#104]
    cmp sp,x2
    b.ne 3f
    .irp n,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30
        mov x2,#(\seed+\n)
        cmp x\n,x2
        b.ne 3f
    .endr
    mov x3,x0
    mov x0,#(\seed+0)
    mov x1,#(\seed+1)
    mov x2,#(\seed+2)
    ldr x4,[x3]
2:  ldr x5,[x3]
    cmp x4,x5
    b.eq 2b
    .irp n,0,1,2
        mov x3,#(\seed+\n)
        cmp x\n,x3
        b.ne 3f
    .endr
    mov x0,#1
    b 4f
3:  mov x0,#0
4:  ldp x18,x19,[sp]
    ldp x20,x21,[sp,#16]
    ldp x22,x23,[sp,#32]
    ldp x24,x25,[sp,#48]
    ldp x26,x27,[sp,#64]
    ldp x28,x29,[sp,#80]
    ldr x30,[sp,#96]
    add sp,sp,#128
    ret
.endm
.pushsection .text.register_probe,"ax"
probe nova_probe_a, 0x1200
probe nova_probe_b, -0x2300
.popsection
