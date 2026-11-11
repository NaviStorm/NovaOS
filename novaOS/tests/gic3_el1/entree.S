// Sonde monoprocesseur chargée en RAM par le banc, sans services UEFI.
.text
.global _start
_start:
    msr daifset, #0xf
    msr spsel, #1
    ldr x0, =0x41000000
    mov sp, x0
    adr x0, vecteurs
    msr vbar_el1, x0
    isb
    bl preparer
    msr daifclr, #2
attente:
    wfi
    b attente

// Seule l'attente ci-dessus peut être interrompue. Aucun registre général
// n'y est vivant. Les IRQ restent masquées pendant le traitement en C.
.balign 2048
vecteurs:
    .rept 5
    b echec
    .balign 128
    .endr
    bl interruption
    eret
    .balign 128
    .rept 10
    b echec
    .balign 128
    .endr
