"""Exerce les fonctions amont de notification sur un anneau split sous EVENT_IDX."""
import argparse
import json
import os
import platform
import subprocess
import tempfile
from pathlib import Path

from virtio_net_rx_modele import TESTS, extraire, lire


def tester(version, sources, manifeste):
    ring = lire(version, sources, "include/standard-headers/linux/virtio_ring.h", manifeste)
    virtio = lire(version, sources, "hw/virtio/virtio.c", manifeste)
    fonctions = ["vring_avail_idx", "vring_avail_ring", "vring_get_used_event",
                 "vring_set_avail_event", "virtio_queue_split_set_notification",
                 "virtio_split_should_notify"]
    with tempfile.TemporaryDirectory() as work:
        tmp = Path(work)
        (tmp / "notification-amont.inc").write_text(
            extraire(ring, r"^static inline int vring_need_event\(", r"^\}") +
            "".join(extraire(virtio, rf"^static [^\n]*\b{nom}\(", r"^\}")
                    for nom in fonctions))
        exe = tmp / "notifications"
        subprocess.run([os.environ.get("CC", "cc"), "-std=gnu11", "-O2",
                        "-I", str(tmp), str(TESTS / "virtio_net_notifications_modele.c"),
                        "-o", str(exe)], check=True)
        resultat = subprocess.check_output([exe], text=True)
    return {name: int(count) for name, count in (word.split("=") for word in resultat.split())}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sources", type=Path)
    parser.add_argument("manifeste", type=Path)
    args = parser.parse_args()
    manifeste = json.loads(args.manifeste.read_text())
    resultats = {v: tester(v, args.sources, manifeste) for v in ("v10.0.12", "v11.1.3")}
    print(json.dumps({"hote": platform.machine(), "portee": "fonctions C, accès simulés, aucune IRQ matérielle",
                      "essais": resultats}, ensure_ascii=False, indent=2))
