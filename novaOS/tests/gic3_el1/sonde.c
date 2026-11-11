/* Profil de banc virt-10.0, un seul PE. Toutes les adresses sont physiques. */
#include <stdint.h>

#define REG8(adresse)  (*(volatile uint8_t *)(uintptr_t)(adresse))
#define REG16(adresse) (*(volatile uint16_t *)(uintptr_t)(adresse))
#define REG32(adresse) (*(volatile uint32_t *)(uintptr_t)(adresse))
#define REG64(adresse) (*(volatile uint64_t *)(uintptr_t)(adresse))
#define ECRIRE_SYS(reg, valeur) __asm__ volatile("msr " #reg ", %0\nisb" :: "r" ((uint64_t)(valeur)) : "memory")
#define GICD 0x08000000u
#define GICR 0x080a0000u
#define SGI (GICR + 0x10000u)
#define TIMER (1u << 30)
#define DISQUE (1u << 5) /* INTID 37 dans la deuxième banque SPI. */
#define PCI (0x4010000000ull + 48 * 4096)
#define BAR4 0x10000000u

static void serie(const char *texte) {
    while (*texte) {
        while (REG32(0x09000018) & 32) {}
        REG32(0x09000000) = *texte++;
    }
}

void echec(void) {
    serie("ECHEC GICv3\n");
    for (;;) __asm__ volatile("wfi");
}

void preparer(void) {
    uint64_t el;
    __asm__ volatile("mrs %0, CurrentEL" : "=r"(el));
    if (el != 4 || REG32(PCI) != 0x10421af4 || REG8(PCI + 0x3d) != 1)
        echec();
    /* BAR du profil sans vecteurs. Décodage mémoire seul, BME à zéro.
       Aucune adresse de file publiée. Le banc change la capacité via QMP. */
    REG32(PCI + 0x20) = BAR4;
    REG32(PCI + 0x24) = 0;
    REG16(PCI + 4) = 2;
    REG8(BAR4 + 20) = 0;
    REG8(BAR4 + 20) = 1;
    REG8(BAR4 + 20) = 3;
    REG32(BAR4) = 1;
    if ((REG32(BAR4 + 4) & 3) != 3) echec();
    REG32(BAR4 + 8) = 1;
    REG32(BAR4 + 12) = 3; /* VERSION_1 et ACCESS_PLATFORM */
    REG8(BAR4 + 20) = 11;
    if (REG8(BAR4 + 20) != 11) echec();
    REG8(BAR4 + 20) = 15;
    if (REG8(BAR4 + 20) != 15 || (REG16(PCI + 4) & 4)) echec();

    REG32(GICR + 0x14) &= ~2u; /* WAKER.ProcessorSleep */
    while (REG32(GICR + 0x14) & 4) {}
    REG32(SGI + 0x80) |= TIMER;          /* IGROUPR0 */
    REG8(SGI + 0x400 + 30) = 0x80;
    REG32(SGI + 0x100) = TIMER;
    REG32(GICD + 0x84) |= DISQUE;
    REG8(GICD + 0x400 + 37) = 0x80;
    /* GIC à un état de sécurité, ARE et groupe 1. Affinité du PE 0. */
    REG32(GICD) = (1u << 4) | 2;
    while (REG32(GICD) & (1u << 31)) {}
    REG64(GICD + 0x6000 + 37 * 8) = 0;
    REG32(GICD + 0x104) = DISQUE;
    __asm__ volatile("dsb sy" ::: "memory");
    ECRIRE_SYS(icc_sre_el1, 1);
    ECRIRE_SYS(icc_ctlr_el1, 0); /* EOI avec désactivation */
    ECRIRE_SYS(icc_pmr_el1, 0xff);
    ECRIRE_SYS(icc_igrpen1_el1, 1);
    ECRIRE_SYS(cntp_tval_el0, 100000);
    ECRIRE_SYS(cntp_ctl_el0, 1);
}

void interruption(void) {
    uint64_t id;
    __asm__ volatile("mrs %0, icc_iar1_el1" : "=r"(id));
    if (id == 30) {
        ECRIRE_SYS(cntp_ctl_el0, 0);
    } else if (id == 37) {
        /* Lire l'ISR abaisse INTA. Le bit 1 identifie la notification. */
        if (!(REG8(BAR4 + 0x1000) & 2) || REG64(BAR4 + 0x2000) != 4096)
            echec();
        REG32(GICD + 0x184) = DISQUE;
        while (REG32(GICD) & (1u << 31)) {}
    } else {
        echec();
    }
    __asm__ volatile("dsb sy" ::: "memory");
    ECRIRE_SYS(icc_eoir1_el1, id);
    if (id == 30) {
        serie("PPI 30 termine\n");
    } else {
        if ((REG32(GICD + 0x104) | REG32(GICD + 0x204) | REG32(GICD + 0x304)) & DISQUE)
            echec();
        if (REG16(PCI + 4) & 4) echec();
        serie("INTx 37 termine et masque\n");
    }
}
