/* Laboratoire Darwin, QEMU TCG API 7, invité monoprocesseur. */
#include <qemu-plugin.h>
#include <mach/mach.h>
#include <pthread.h>
#include <pthread/qos.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static uint64_t entree, marker, taille_marker, actif, phases, barrieres;
static qos_class_t classe;
static FILE *sortie;
static bool installe, ouvert;

struct point {
    char type;
    unsigned indice;
    uint64_t fil, cpu_ns, ecoule_ns;
    size_t coeur;
    qos_class_t demandee;
    int relative, base, courante;
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

static void relever(char type, unsigned indice)
{
    if (nombre == sizeof(points) / sizeof(*points)) {
        abort();
    }
    struct point *p = &points[nombre++];
    p->type = type;
    p->indice = indice;
    p->ecoule_ns = horloge(CLOCK_MONOTONIC);
    p->cpu_ns = horloge(CLOCK_THREAD_CPUTIME_ID);
    pthread_t fil = pthread_self();
    thread_extended_info_data_t info;
    mach_msg_type_number_t n = THREAD_EXTENDED_INFO_COUNT;
    if (pthread_cpu_number_np(&p->coeur) || pthread_threadid_np(NULL, &p->fil) ||
        pthread_get_qos_class_np(fil, &p->demandee, &p->relative) ||
        thread_info(pthread_mach_thread_np(fil), THREAD_EXTENDED_INFO, (thread_info_t)&info, &n)
            != KERN_SUCCESS) {
        abort();
    }
    p->base = info.pth_priority;
    p->courante = info.pth_curpri;
}

static void initialiser(unsigned vcpu, void *opaque)
{
    if (installe) {
        return;
    }
    relever('I', 0);
    if (classe && pthread_set_qos_class_self_np(classe, 0)) {
        abort();
    }
    relever('Q', 0);
    installe = true;
}

static void memoriser(unsigned vcpu, qemu_plugin_meminfo_t info, uint64_t adresse, void *opaque)
{
    if (adresse == actif) {
        qemu_plugin_mem_value v = qemu_plugin_mem_get_value(info);
        if (v.type != QEMU_PLUGIN_MEM_VALUE_U8 || !installe) {
            abort();
        }
        bool nouvelle = v.data.u8 != 0;
        if (nouvelle == ouvert) {
            return;
        }
        ouvert = nouvelle;
        relever(ouvert ? 'S' : 'T', 0);
        if (!ouvert) {
            for (size_t i = 0; i < nombre; i++) {
                struct point *p = &points[i];
                fprintf(sortie, "%c %u %" PRIu64 " %zu %u %d %d %d %" PRIu64 " %" PRIu64 "\n",
                        p->type, p->indice, p->fil, p->coeur, (unsigned)p->demandee,
                        p->relative, p->base, p->courante, p->cpu_ns, p->ecoule_ns);
            }
            if (fflush(sortie)) {
                abort();
            }
        }
    } else if (ouvert && adresse >= phases && adresse - phases < 9 * 8) {
        relever('E', (unsigned)((adresse - phases) / 8));
    } else if (ouvert && adresse >= barrieres && adresse - barrieres < 4 * 8) {
        relever('B', (unsigned)((adresse - barrieres) / 8));
    }
}

static void traduire(struct qemu_plugin_tb *tb, void *opaque)
{
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); i++) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        uint64_t pc = qemu_plugin_insn_vaddr(insn);
        if (pc == entree) {
            qemu_plugin_register_vcpu_insn_exec_cb(insn, initialiser, QEMU_PLUGIN_CB_NO_REGS, NULL);
        }
        if (pc >= marker && pc - marker < taille_marker) {
            qemu_plugin_register_vcpu_mem_cb(insn, memoriser, QEMU_PLUGIN_CB_NO_REGS,
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
    uint64_t *args[] = {&entree, &marker, &taille_marker, &actif, &phases, &barrieres};
    for (size_t i = 0; i < 6; i++) {
        char *fin;
        if (strncmp(argv[i], "addr=", 5)) {
            return -1;
        }
        errno = 0;
        *args[i] = strtoull(argv[i] + 5, &fin, 16);
        if (errno || fin == argv[i] + 5 || *fin || !*args[i]) {
            return -1;
        }
    }
    if (!strcmp(argv[6], "class=background")) {
        classe = QOS_CLASS_BACKGROUND;
    } else if (!strcmp(argv[6], "class=user-initiated")) {
        classe = QOS_CLASS_USER_INITIATED;
    } else if (strcmp(argv[6], "class=observe")) {
        return -1;
    }
    if (strncmp(argv[7], "out=", 4)) {
        return -1;
    }
    sortie = fopen(argv[7] + 4, "wx");
    if (!sortie) {
        return -1;
    }
    qemu_plugin_register_vcpu_tb_trans_cb(id, traduire, NULL);
    return 0;
}
