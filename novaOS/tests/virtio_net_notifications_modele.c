/* Banc hôte pour split virtqueue de 16 entrées. Les champs partagés sont
 * accessibles au modèle uniquement par les accesseurs instrumentés ci-dessous. */
#include <assert.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef size_t hwaddr;
typedef struct { uint16_t flags, idx, ring[]; } VRingAvail;
typedef struct { uint32_t id, len; } VRingUsedElem;
typedef struct { uint16_t flags, idx; VRingUsedElem ring[]; } VRingUsed;
typedef struct { bool ecriture; uint16_t mots[68]; } Page;
typedef struct { Page avail, used; } VRingMemoryRegionCaches;
typedef struct { bool event_idx; } VirtIODevice;
typedef struct {
    VirtIODevice *vdev;
    struct { int num; } vring;
    uint16_t shadow_avail_idx, signalled_used, used_idx, inuse;
    bool notification, signalled_used_valid;
} VirtQueue;

static VRingMemoryRegionCaches pages = { .used.ecriture = true };
static unsigned lectures, ecritures;
#define vring_get_region_caches(vq) (&pages)
#define VIRTIO_RING_F_EVENT_IDX 29
#define VIRTIO_F_NOTIFY_ON_EMPTY 24
#define VRING_USED_F_NO_NOTIFY 1
#define VRING_AVAIL_F_NO_INTERRUPT 1
#define virtio_vdev_has_feature(vdev, bit) ((bit) == VIRTIO_RING_F_EVENT_IDX && (vdev)->event_idx)
#define RCU_READ_LOCK_GUARD() ((void)0)
#define smp_mb() ((void)0)
#define virtio_queue_empty(vq) false
#define vring_used_flags_unset_bit(...) abort()
#define vring_used_flags_set_bit(...) abort()
#define vring_avail_flags(...) (abort(), 0)

static uint16_t virtio_lduw_phys_cached(VirtIODevice *dev, Page *page, hwaddr offset) {
    assert(dev->event_idx && page == &pages.avail && !page->ecriture);
    assert(offset == 2 || offset == 36);
    lectures++;
    return page->mots[offset / 2];
}
static void virtio_stw_phys_cached(VirtIODevice *dev, Page *page, hwaddr offset, uint16_t v) {
    assert(dev->event_idx && page == &pages.used && page->ecriture && offset == 132);
    ecritures++;
    page->mots[offset / 2] = v;
}
static void address_space_cache_invalidate(Page *p, hwaddr offset, size_t n) {
    assert(p == &pages.used && offset == 132 && n == 2);
}

#include "notification-amont.inc"

int main(void) {
    VirtIODevice dev = { .event_idx = true };
    VirtQueue q = { .vdev = &dev, .vring.num = 16 };
    unsigned essais = 0, franchissements = 0;
    for (unsigned ancien = 0; ancien < 65536; ancien++) {
        for (unsigned nombre = 0; nombre <= 16; nombre++) {
            for (int relatif = -1; relatif <= 17; relatif++) {
                q.used_idx = (uint16_t)(ancien + nombre);
                q.signalled_used = (uint16_t)ancien;
                q.signalled_used_valid = true;
                pages.avail.mots[36 / 2] = (uint16_t)((int)ancien + relatif);
                bool attendu = relatif >= 0 && (unsigned)relatif < nombre;
                assert(virtio_split_should_notify(&dev, &q) == attendu);
                assert(q.signalled_used == q.used_idx);
                essais++;
                if (ancien + nombre > 65535) franchissements++;
            }
        }
        for (unsigned on = 0; on < 2; on++) {
            q.notification = on;
            unsigned avant = ecritures;
            pages.avail.mots[1] = (uint16_t)ancien;
            virtio_queue_split_set_notification(&q, on);
            assert(ecritures == avant + on);
            if (on) assert(pages.used.mots[132 / 2] == ancien);
        }
    }
    q.signalled_used_valid = false;
    assert(virtio_split_should_notify(&dev, &q));
    printf("essais_decision=%u franchissements=%u lectures_avail=%u ecritures_used=%u\n",
           essais, franchissements, lectures, ecritures);
}
