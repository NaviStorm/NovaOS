/* Banc hôte du modèle, sans VM. Une seule trame, une file et un tampon. */
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
static struct iovec sg = { tampon, sizeof(tampon) };
static VirtQueueElement element = { &sg, 1 };
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
#define virtio_net_has_buffers(q, size) ((size) <= sizeof(tampon))
#define receive_filter(n, buf, size) true
#define g_free(p) assert((p) == &element)
#define cpu_to_le16(v) (v)

/* Toute branche hors du profil fait échouer le témoin. */
#define virtio_net_process_rss(...) (abort(), 0)
#define qemu_get_subqueue(...) (abort(), (NetClientState *)NULL)
#define virtio_net_hdr_swap(...) abort()
#define work_around_broken_dhclient(...) abort()
#define virtio_error(...) abort()
#define virtqueue_detach_element(...) abort()
#define virtqueue_unpop(...) abort()

static VirtQueueElement *virtqueue_pop(int *q, size_t taille) {
    assert(q == &numero && taille == sizeof(element) && ++extractions == 1);
    return &element;
}
static void virtqueue_fill(int *q, VirtQueueElement *e, size_t n, size_t j) {
    assert(q == &numero && e == &element && j == 0 && ++completions == 1);
    longueur = n;
}
static void virtqueue_flush(int *q, size_t n) {
    assert(q == &numero && n == 1 && completions == 1 && ++publications == 1);
}
static void virtio_notify(VirtIODevice *v, int *q) {
    assert(v && q == &numero && publications == 1 && ++notifications == 1);
}
static size_t iov_from_buf(const struct iovec *v, unsigned count,
                           size_t offset, const void *data, size_t length) {
    assert(count == 1 && offset <= v->iov_len && length <= v->iov_len - offset);
    uint8_t *dest = (uint8_t *)v->iov_base + offset;
    assert(dest >= tampon && dest + length <= tampon + sizeof(tampon));
    printf("copie offset=%zu octets=%zu\n", (size_t)(dest - tampon), length);
    memcpy(dest, data, length);
    return length;
}
static unsigned iov_copy(struct iovec *dst, unsigned cap, const struct iovec *src,
                         unsigned count, size_t offset, size_t len) {
    assert(cap && count == 1 && offset <= src->iov_len && len <= src->iov_len - offset);
    dst[0] = (struct iovec){(uint8_t *)src->iov_base + offset, len};
    return 1;
}
static void virtio_stw_p(VirtIODevice *v, uint16_t *dest, uint16_t value) {
    assert(v);
    uint8_t le[2] = { value & 255, value >> 8 };
    memcpy(dest, le, 2);
}

#include "reception-amont.inc"

int main(void) {
    uint8_t trame[60];
    for (size_t i = 0; i < sizeof(trame); i++) trame[i] = (uint8_t)(i + 1);
    struct Cas { const char *nom; bool moderne, fusion; uint8_t initial; };
    static const struct Cas cas[] = {
        {"moderne-simple", true, false, 0}, {"témoin-mémoire", true, false, 0x5a},
        {"moderne-fusion", true, true, 0}, {"legacy-simple", false, false, 0}
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
        completions = notifications = publications = extractions = 0;
        longueur = 0;
        assert(virtio_net_receive_rcu(&n, trame, sizeof(trame)) == sizeof(trame));
        assert(longueur == sizeof(trame) + n.guest_hdr_len && notifications == 1);
        assert(memcmp(tampon + n.guest_hdr_len, trame, sizeof(trame)) == 0);
        if (c->moderne || c->fusion) {
            printf("cas=%s num_buffers=%u longueur=%zu contenu=conforme\n", c->nom,
                   (unsigned)(tampon[10] | (tampon[11] << 8)), longueur);
        } else {
            printf("cas=%s longueur=%zu contenu=conforme\n", c->nom, longueur);
        }
    }
    return 0;
}
