// Sonde de laboratoire UEFI x86_64. Le firmware fournit le protocole Block I/O.
// ABI publique UEFI, sections 4.3, 7.3 et 13.9. Aucune dépendance au noyau.
#include <stddef.h>
#include <stdint.h>

enum { BLOCK = 4096, BLOCKS = 320 };
typedef uint64_t Status;
typedef struct { uint32_t a; uint16_t b, c; uint8_t d[8]; } Guid;
typedef struct {
    uint32_t id;
    uint8_t removable, present, partition, read_only, write_caching;
    uint32_t block_size, alignment;
    uint64_t last_block;
} Media;
typedef struct BlockIo BlockIo;
struct BlockIo {
    uint64_t revision;
    Media *media;
    void *reset;
    Status (*read)(BlockIo *, uint32_t, uint64_t, size_t, void *);
    Status (*write)(BlockIo *, uint32_t, uint64_t, size_t, void *);
    Status (*flush)(BlockIo *);
};
typedef struct {
    uint8_t header[24];
    void *before_handle_protocol[16];
    Status (*handle_protocol)(void *, const Guid *, void **);
    void *reserved, *register_notify;
    Status (*locate_handle)(uint32_t, const Guid *, void *, size_t *, void **);
    void *before_connect_controller[10];
    Status (*connect_controller)(void *, void **, void *, uint8_t);
} BootServices;
typedef struct {
    uint8_t before_boot_services[96];
    BootServices *boot;
} SystemTable;

_Static_assert(offsetof(Media, block_size) == 12 && offsetof(Media, last_block) == 24, "EFI_BLOCK_IO_MEDIA");
_Static_assert(offsetof(BootServices, locate_handle) == 176, "EFI_BOOT_SERVICES");
_Static_assert(offsetof(BootServices, connect_controller) == 264, "EFI_BOOT_SERVICES");
static const Guid block_io = {0x964e5b21, 0x6459, 0x11d2, {0x8e,0x39,0x00,0xa0,0xc9,0x69,0x72,0x3b}};
static const Guid pci_io = {0x4cf5b200, 0x68b8, 0x4ca5, {0x9e,0xec,0xb2,0x3e,0x3f,0x50,0x02,0x9a}};
static _Alignas(BLOCK) uint8_t buffer[BLOCK];

typedef struct { uint32_t block; uint8_t data[BLOCK]; } Operation;
extern const Operation novafs_plan[];

static void debug(const char *message) {
    while (*message) {
        __asm__ volatile("outb %0, $0xe9" : : "a"(*message++));
    }
}

static void debug_hex(uint64_t value) {
    for (int i = 60; i >= 0; i -= 4) {
        uint8_t digit = "0123456789abcdef"[(value >> i) & 15];
        __asm__ volatile("outb %0, $0xe9" : : "a"(digit));
    }
}

static void finish(const char *message) {
    debug(message);
    // Aucune E/S ni sortie EFI après le marqueur. L'hôte envoie SIGKILL.
    for (;;) {
        __asm__ volatile("cli; hlt");
    }
}

static int is_fixture(BlockIo *disk) {
    Media *m = disk->media;
    debug("secteur="); debug_hex(m->block_size);
    debug(" dernier="); debug_hex(m->last_block);
    debug(" cache="); debug_hex(m->write_caching);
    debug(" ro="); debug_hex(m->read_only); debug("\n");
    if (!m->present || m->partition || m->read_only || !m->write_caching ||
        m->block_size != PLAN_SECTOR || m->last_block != BLOCKS * (BLOCK / PLAN_SECTOR) - 1 ||
        (m->alignment && (uintptr_t)buffer % m->alignment != 0)) {
        return 0;
    }
    if (disk->read(disk, m->id, 0, BLOCK, buffer) != 0) {
        return 0;
    }
    for (size_t i = 0; i < 8; i++) {
        if (buffer[i] != "NOVAP0FS"[i]) return 0;
    }
    for (size_t i = 0; i < 16; i++) {
        if (buffer[16+i] != 0x31 || buffer[32+i] != 0x52) return 0;
    }
    return buffer[12] == 1 && buffer[13] == 0;
}

Status efi_main(void *image, SystemTable *system) {
    (void)image;
    void *handles[64];
    size_t bytes = sizeof(handles);
    BootServices *bs = system->boot;
    // Le gestionnaire d'amorçage peut n'avoir connecté que le disque de l'ESP.
    if (bs->locate_handle(2, &pci_io, 0, &bytes, handles) != 0 || bytes > sizeof(handles)) {
        finish("ERREUR enumeration PCI\n");
    }
    for (size_t i = 0; i < bytes / sizeof(handles[0]); i++) {
        bs->connect_controller(handles[i], 0, 0, 1);
    }
    bytes = sizeof(handles);
    if (bs->locate_handle(2 /* ByProtocol */, &block_io, 0, &bytes, handles) != 0 ||
        bytes > sizeof(handles) || bytes % sizeof(handles[0])) {
        finish("ERREUR enumeration\n");
    }
    BlockIo *target = 0;
    for (size_t i = 0; i < bytes / sizeof(handles[0]); i++) {
        void *interface = 0;
        if (bs->handle_protocol(handles[i], &block_io, &interface) == 0 && is_fixture(interface)) {
            if (target) finish("ERREUR identite ambigue\n");
            target = interface;
        }
    }
    if (!target) finish("ERREUR disque, geometrie ou cache\n");
    for (size_t i = 0; i < PLAN_RECORDS; i++) {
        const Operation *op = &novafs_plan[i];
        if (op->block == UINT32_MAX) {
            if (target->flush(target)) finish("ERREUR flush UEFI\n");
        } else {
            if (op->block >= BLOCKS) finish("ERREUR LBA\n");
            for (size_t j = 0; j < BLOCK; j++) buffer[j] = op->data[j];
            uint64_t lba = (uint64_t)op->block * (BLOCK / PLAN_SECTOR);
            if (target->write(target, target->media->id, lba, BLOCK, buffer)) {
                finish("ERREUR ecriture UEFI\n");
            }
        }
    }
    finish("NOVAFS-P1-DONE\n");
    return 0;
}
