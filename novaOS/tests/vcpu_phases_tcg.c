/* Sonde hôte indépendante pour les ELF de laboratoire et QEMU 11 (API 7). */
#include <qemu-plugin.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static qemu_plugin_u64 compteur;
static uint64_t marker, taille, actif, entrees, durees, barrieres;
static bool ouvert, compter;
static FILE *sortie;

/* Une seule session de publication et un seul vCPU par lancement. */
struct point {
    char type;
    unsigned indice;
    uint64_t instructions, cpu_ns, ecoule_ns;
};
static struct point points[2048];
static size_t nombre;

static uint64_t horloge(clockid_t id)
{
    struct timespec ts;
    if (clock_gettime(id, &ts)) {
        abort();
    }
    return (uint64_t)ts.tv_sec * 1000000000 + (uint64_t)ts.tv_nsec;
}

static void ecriture(unsigned int cpu, qemu_plugin_meminfo_t info,
                     uint64_t adresse, void *opaque)
{
    char type;
    unsigned indice = 0;
    if (adresse == actif) {
        qemu_plugin_mem_value valeur = qemu_plugin_mem_get_value(info);
        if (valeur.type != QEMU_PLUGIN_MEM_VALUE_U8) {
            abort();
        }
        type = valeur.data.u8 ? 'S' : 'T';
        if ((type == 'S') == ouvert) {
            return;
        }
        ouvert = type == 'S';
    } else if (!ouvert) {
        return;
    } else if (adresse >= entrees && adresse - entrees < 9 * 8) {
        type = 'E';
        indice = (adresse - entrees) / 8;
    } else if (adresse >= durees && adresse - durees < 9 * 8) {
        type = 'F';
        indice = (adresse - durees) / 8;
    } else if (adresse >= barrieres && adresse - barrieres < 4 * 8) {
        type = 'B';
        indice = (adresse - barrieres) / 8;
    } else {
        return;
    }
    if (nombre == sizeof(points) / sizeof(points[0])) {
        abort();
    }
    points[nombre++] = (struct point){type, indice, compter ? qemu_plugin_u64_get(compteur, cpu) : 0,
                                     horloge(CLOCK_THREAD_CPUTIME_ID), horloge(CLOCK_MONOTONIC)};
    if (type == 'T') {
        for (size_t i = 0; i < nombre; i++) {
            const struct point *p = &points[i];
            fprintf(sortie, "%c %u %" PRIu64 " %" PRIu64 " %" PRIu64 "\n", p->type,
                    p->indice, p->instructions, p->cpu_ns, p->ecoule_ns);
        }
        if (fflush(sortie)) {
            abort();
        }
    }
}

static void traduire(struct qemu_plugin_tb *tb, void *opaque)
{
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); i++) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        uint64_t pc = qemu_plugin_insn_vaddr(insn);
        if (compter) {
            qemu_plugin_register_vcpu_insn_exec_inline_per_vcpu(insn, QEMU_PLUGIN_INLINE_ADD_U64,
                                                                compteur, 1);
        }
        if (pc >= marker && pc - marker < taille) {
            qemu_plugin_register_vcpu_mem_cb(insn, ecriture, QEMU_PLUGIN_CB_NO_REGS,
                                             QEMU_PLUGIN_MEM_W, NULL);
        }
    }
}

QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id, const qemu_info_t *info,
                                           int argc, char **argv)
{
    if (!info->system_emulation || info->system.max_vcpus != 1 || argc != 8) {
        return -1;
    }
    uint64_t *param[] = {&marker, &taille, &actif, &entrees, &durees, &barrieres};
    for (unsigned i = 0; i < 6; i++) {
        char *fin;
        errno = 0;
        if (strncmp(argv[i], "addr=", 5) != 0) {
            return -1;
        }
        *param[i] = strtoull(argv[i] + 5, &fin, 16);
        if (errno || fin == argv[i] + 5 || *fin || !*param[i]) {
            return -1;
        }
    }
    if (strncmp(argv[6], "out=", 4) != 0) {
        return -1;
    }
    if (strcmp(argv[7], "instructions=on") && strcmp(argv[7], "instructions=off")) {
        return -1;
    }
    compter = !strcmp(argv[7], "instructions=on");
    sortie = fopen(argv[6] + 4, "wx");
    if (!sortie) {
        return -1;
    }
    compteur = qemu_plugin_scoreboard_u64(qemu_plugin_scoreboard_new(sizeof(uint64_t)));
    qemu_plugin_register_vcpu_tb_trans_cb(id, traduire, NULL);
    return 0;
}
