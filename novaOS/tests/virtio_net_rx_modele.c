/* Banc hôte du modèle, sans VM. Segments et entrées de file distincts. */
#include <assert.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/uio.h>

typedef uint16_t __virtio16;
#include "entetes-amont.inc"

_Static_assert(sizeof(struct virtio_net_hdr) == 10, "entete historique");
_Static_assert(sizeof(struct virtio_net_hdr_v1) == 12, "entete moderne");
_Static_assert(offsetof(struct virtio_net_hdr_v1, num_buffers) == 10, "champ RX");

typedef struct { struct iovec *in_sg; unsigned in_num; } VirtQueueElement;
typedef struct { int *rx_vq; } VirtIONetQueue;
typedef struct {
    struct { bool enabled, enabled_software_rss, populate_hash; } rss_data;
    bool has_vnet_hdr, needs_vnet_hdr_swap, mergeable_rx_bufs, version_1;
    size_t host_hdr_len, guest_hdr_len;
    unsigned curr_queue_pairs;
    void *nic;
} VirtIONet;
typedef VirtIONet VirtIODevice;
typedef VirtIONet NetClientState;

static uint8_t tampon[2048];
static struct iovec segments[3];
static VirtQueueElement elements[2];
static unsigned disponibles, retours;
static int numero;
static VirtIONetQueue file = { &numero };
static unsigned completions, notifications, publications, extractions;
static size_t longueur;

#define VIRTIO_F_VERSION_1 32
#define virtio_vdev_has_feature(v, b) ((b) == VIRTIO_F_VERSION_1 && (v)->version_1)
#define QEMU_UNINITIALIZED
#define VIRTQUEUE_MAX_SIZE 16
#define ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))
#define VIRTIO_DEVICE(n) (n)
#define qemu_get_nic_opaque(nc) (nc)
#define virtio_net_get_subqueue(nc) (&file)
#define virtio_net_can_receive(nc) true
#define virtio_net_has_buffers(q, size) (disponibles != 0)
#define receive_filter(n, buf, size) true
#define g_free(p) assert((p) == &elements[0] || (p) == &elements[1])
#define cpu_to_le16(v) (v)

/* Toute branche hors du profil fait échouer le témoin. */
#define virtio_net_process_rss(...) (abort(), 0)
#define qemu_get_subqueue(...) (abort(), (NetClientState *)NULL)
#define virtio_net_hdr_swap(...) abort()
#define work_around_broken_dhclient(...) abort()
#define virtio_error(...) abort()
#define virtqueue_detach_element(...) abort()
static void virtqueue_unpop(int *q, VirtQueueElement *elem, size_t len) {
    assert(q == &numero && elem == &elements[0] && len == 32 && retours++ == 0);
}

static VirtQueueElement *virtqueue_pop(int *q, size_t taille) {
    assert(q == &numero && taille == sizeof(elements[0]));
    return extractions < disponibles ? &elements[extractions++] : NULL;
}
static void virtqueue_fill(int *q, VirtQueueElement *e, size_t n, size_t j) {
    assert(q == &numero && j == completions && j < disponibles && e == &elements[j]);
    completions++;
    longueur += n;
}
static void virtqueue_flush(int *q, size_t n) {
    assert(q == &numero && n == completions && n > 0 && ++publications == 1);
}
static void virtio_notify(VirtIODevice *v, int *q) {
    assert(v && q == &numero && publications == 1 && ++notifications == 1);
}
static unsigned iov_copy(struct iovec *dst, unsigned cap, const struct iovec *src,
                         unsigned count, size_t offset, size_t len) {
    unsigned used = 0;
    for (unsigned i = 0; i < count && len > 0; i++) {
        if (offset >= src[i].iov_len) {
            offset -= src[i].iov_len;
            continue;
        }
        size_t n = src[i].iov_len - offset;
        if (n > len) n = len;
        assert(used < cap);
        dst[used++] = (struct iovec){(uint8_t *)src[i].iov_base + offset, n};
        offset = 0;
        len -= n;
    }
    return used;
}
static size_t iov_from_buf(const struct iovec *v, unsigned count,
                           size_t offset, const void *data, size_t length) {
    struct iovec portions[3];
    unsigned n = iov_copy(portions, 3, v, count, offset, length);
    size_t total = 0;
    for (unsigned j = 0; j < n; j++) {
        uint8_t *dest = portions[j].iov_base;
        size_t len = portions[j].iov_len;
        assert(dest >= tampon && dest + len <= tampon + sizeof(tampon));
        printf("copie offset=%zu octets=%zu\n", (size_t)(dest - tampon), len);
        memcpy(dest, (const uint8_t *)data + total, len);
        total += len;
    }
    return total;
}
static void virtio_stw_p(VirtIODevice *v, uint16_t *dest, uint16_t value) {
    assert(v);
    uint8_t le[2] = { value & 255, value >> 8 };
    memcpy(dest, le, 2);
}

#include "reception-amont.inc"

int main(void) {
    uint8_t trame[1514];
    for (size_t i = 0; i < sizeof(trame); i++) trame[i] = (uint8_t)(i + 1);
    struct Cas { const char *nom; bool moderne, fusion; uint8_t initial; size_t longueur; unsigned coupure, suivant; };
    static const struct Cas cas[] = {
        {"moderne-simple", true, false, 0, 60}, {"témoin-mémoire", true, false, 0x5a, 60},
        {"moderne-fusion", true, true, 0, 60}, {"legacy-simple", false, false, 0, 60},
        {"fusion-1514", true, true, 0, 1514}, {"simple-1514", true, false, 0, 1514},
        {"sg-10", true, false, 0x5a, 60, 10}, {"sg-11", true, false, 0x5a, 60, 11},
        {"legacy-fusion", false, true, 0x5a, 60, 11},
        {"fusion-deux", true, true, 0x5a, 60, 11, 32},
        {"partiel", true, false, 0x5a, 60, 11, 32},
        {"sans-tampon", true, false, 0x5a, 60}
    };
    for (size_t i = 0; i < sizeof(cas) / sizeof(cas[0]); i++) {
        const struct Cas *c = &cas[i];
        VirtIONet n = {
            .guest_hdr_len = c->moderne || c->fusion ? sizeof(struct virtio_net_hdr_v1)
                                                     : sizeof(struct virtio_net_hdr),
            .mergeable_rx_bufs = c->fusion,
            .version_1 = c->moderne
        };
        memset(tampon, c->initial, sizeof(tampon));
        completions = notifications = publications = extractions = retours = 0;
        size_t fin = c->suivant ? c->suivant : sizeof(tampon);
        segments[0] = (struct iovec){tampon, c->coupure ? c->coupure : fin};
        segments[1] = (struct iovec){tampon + c->coupure, fin - c->coupure};
        segments[2] = (struct iovec){tampon + fin, sizeof(tampon) - fin};
        elements[0] = (VirtQueueElement){segments, c->coupure ? 2 : 1};
        elements[1] = (VirtQueueElement){segments + 2, 1};
        disponibles = c->suivant && c->fusion ? 2 : 1;
        longueur = 0;
        if (strcmp(c->nom, "sans-tampon") == 0) {
            disponibles = 0;
            assert(virtio_net_receive_rcu(&n, trame, c->longueur) == 0);
            assert(extractions == 0 && completions == 0 && notifications == 0);
            for (size_t j = 0; j < sizeof(tampon); j++) assert(tampon[j] == c->initial);
            puts("cas=sans-tampon aucune-completion");
            continue;
        }
        assert(virtio_net_receive_rcu(&n, trame, c->longueur) == (ssize_t)c->longueur);
        if (c->suivant && !c->fusion) {
            assert(retours == 1 && completions == 0 && notifications == 0 && publications == 0);
            assert(tampon[10] == c->initial && tampon[11] == c->initial);
            for (size_t j = fin; j < sizeof(tampon); j++) assert(tampon[j] == c->initial);
            puts("cas=partiel abandon aucune-completion");
            continue;
        }
        assert(completions == disponibles);
        for (size_t j = longueur; j < sizeof(tampon); j++) assert(tampon[j] == c->initial);
        assert(longueur == c->longueur + n.guest_hdr_len && notifications == 1);
        assert(memcmp(tampon + n.guest_hdr_len, trame, c->longueur) == 0);
        if (c->moderne || c->fusion) {
            printf("cas=%s num_buffers=%u longueur=%zu contenu=conforme\n", c->nom,
                   (unsigned)(tampon[10] | (tampon[11] << 8)), longueur);
        } else {
            printf("cas=%s longueur=%zu contenu=conforme\n", c->nom, longueur);
        }
    }
    return 0;
}
