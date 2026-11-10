// Même format de données que le programme x86_64. Aucun appel au noyau dans la boucle.
.section .text.demo,"ax"
.global _start
_start:
    .irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30
        cbnz x\n,bad
    .endr
    mrs x0,tpidr_el0
    cbnz x0,bad
    mov x1,#0x400000
    add x1,x1,#1,lsl #12
    ldr x2,[x1,#16]
    ldr x3,[x1]
    cmp x3,#15
    b.eq bad_return_sp
    cmp x3,#16
    b.eq bad_return_pc
    cmp x3,#11
    b.hs abi_probe
    cmp x3,#9
    b.eq unknown_entry
    cmp x3,#10
    b.eq invalid_stack
    cmp x3,#1
    b.eq read_bad
    cmp x3,#2
    b.eq write_bad
    cmp x3,#8
    b.eq write_bad
    cmp x3,#3
    b.eq exec_bad
    cmp x3,#4
    b.eq privileged
    cmp x3,#5
    b.eq quick
    cmp x3,#6
    b.eq forbidden_fp
    ldr x4,[x1,#8]
    str x4,[sp,#-16]!
    msr tpidr_el0,x4
    mov x5,#0x600000
    .irp n,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30
        mov x\n,x4
    .endr
again:
    ldr x6,[sp]
    cmp x6,x4
    b.ne bad
    mrs x6,tpidr_el0
    cmp x6,x4
    b.ne bad
    ldr x6,[x1,#8]
    cmp x6,x4
    b.ne bad
    .irp n,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30
        cmp x\n,x4
        b.ne bad
    .endr
    cmp x3,#7
    b.ne count
    ldr x6,[x5]
    cmp x6,x4
    b.ne bad
count:
    ldr x6,[x1,#24]
    add x6,x6,#1
    str x6,[x1,#24]
    ldr x6,[x1,#32]
    cbz x6,again
quick:
    ldr x0,[x1,#8]
    mov x8,#0x10000
    add x8,x8,#4
    svc #0x4e
read_bad:
    ldr x0,[x2]
    b bad
write_bad:
    strb wzr,[x2]
    b bad
exec_bad:
    br x2
unknown_entry:
    svc #0x52
    b bad
invalid_stack:
    mov sp,x2
    str xzr,[sp]
    b bad
privileged:
    mrs x0,ttbr1_el1
    b bad
forbidden_fp:
    .inst 0x4f00e400
bad:
    mov x0,#255
    mov x8,#0x10000
    add x8,x8,#4
    svc #0x4e
    brk #0

abi_probe:
    mov x0,sp
    str x0,[sp,#-16]!
    .irp n,2,3,4,5,6,7,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30
        mov x\n,#0x5201
    .endr
    msr tpidr_el0,x2
    mov x8,#0x10000
    add x8,x8,#3
    cmp x2,x2
    svc #0x4e
    b.ne bad
    cbnz x0,bad
    cbnz x1,bad
    mov x0,#0x5201
    .irp n,2,3,4,5,6,7,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30
        cmp x\n,x0
        b.ne bad
    .endr
    mrs x1,tpidr_el0
    cmp x1,x0
    b.ne bad
    ldr x1,[sp],#16
    cmp sp,x1
    b.ne bad
    bl nova_abi_probe
    mov x8,#0x10000
    add x8,x8,#4
    svc #0x4e

bad_return_sp:
    mov x8,#0x10000
    add x8,x8,#1
    mov x9,#0xffffffffffffffff
    mov sp,x9
    svc #0x4e
    brk #0
bad_return_pc:
    mov x8,#0x10000
    add x8,x8,#1
    b .Ledge
.section .user_edge,"ax"
.Ledge:
    svc #0x4e
