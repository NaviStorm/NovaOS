/* SPDX-License-Identifier: MIT
 * Témoin TCG externe au firmware. Il observe toutes les écritures du vCPU.
 * Le profil Q35 utilise une DIMM de laboratoire au premier octet au-delà de 4
 * Gio.
 */
#include <inttypes.h>
#include <qemu-plugin.h>
#include <stdio.h>
#include <string.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static FILE *journal;
static const uint64_t reserve = UINT64_C(1) << 32;
enum { page = 4096 };

static void store(unsigned cpu, qemu_plugin_meminfo_t info, uint64_t va,
                  void *opaque) {
  struct qemu_plugin_hwaddr *hw = qemu_plugin_get_hwaddr(info, va);
  if (!hw) {
    fputs("adresse-inconnue\n", journal);
    fflush(journal);
    return;
  }
  uint64_t addr = qemu_plugin_hwaddr_phys_addr(hw);
  uint64_t size = UINT64_C(1) << qemu_plugin_mem_size_shift(info);
  if (addr < reserve + 24 * page && addr + size > reserve &&
      (addr < reserve + 14 * page || addr + size > reserve + 20 * page)) {
    fprintf(journal,
            "table cpu=%u pc=%" PRIxPTR " gpa=%" PRIx64 " octets=%" PRIu64 "\n",
            cpu, (uintptr_t)opaque, addr, size);
    fflush(journal);
  } else if (addr == reserve + 24 * page + 27 * 8 ||
             addr == reserve + 24 * page + 29 * 8) {
    qemu_plugin_mem_value value = qemu_plugin_mem_get_value(info);
    if (value.type != QEMU_PLUGIN_MEM_VALUE_U64) {
      fputs("marqueur-invalide\n", journal);
    } else {
      fprintf(journal, "marqueur cpu=%u cellule=%u valeur=%" PRIu64 "\n", cpu,
              (unsigned)((addr - reserve - 24 * page) / 8), value.data.u64);
    }
    fflush(journal);
  }
}
static void instrument(struct qemu_plugin_tb *tb, void *unused) {
  (void)unused;
  for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); i++) {
    struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
    qemu_plugin_register_vcpu_mem_cb(
        insn, store, QEMU_PLUGIN_CB_NO_REGS, QEMU_PLUGIN_MEM_W,
        (void *)(uintptr_t)qemu_plugin_insn_vaddr(insn));
  }
}
QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id,
                                           const qemu_info_t *info, int argc,
                                           char **argv) {
  (void)argv;
  if (argc || !info->system_emulation || strcmp(info->target_name, "x86_64") ||
      info->system.smp_vcpus != 1)
    return -1;
  journal = fopen("cpu.log", "wx");
  if (!journal)
    return -1;
  fputs("initialise\n", journal);
  fflush(journal);
  qemu_plugin_register_vcpu_tb_trans_cb(id, instrument, NULL);
  return 0;
}
