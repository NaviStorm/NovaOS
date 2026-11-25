"""Rejoue la chronologie du banc DXE/GICv3 à partir de ses observations archivées."""
import argparse
from collections import Counter
import json
from pathlib import Path
import re


def verifier(archive):
    for phase in ("reset", "fin"):
        state = json.loads((archive / f"{phase}.json").read_bytes())
        if state["vm"]["running"] or state["boot_bypass"] is not False or \
           state["iommu_platform"] is not True or state["pci_id_ecam"] != 0x10411af4 or state["pci_command"] & 4:
            raise ValueError(f"Observation PCI/IOMMU inattendue à {phase}")
    trace = (archive / "qemu.trace").read_text().splitlines()
    active = {}
    counts = Counter()
    route = False
    disk = False
    bme = []
    for i, s in enumerate(trace, 1):
        if s.startswith("virtio_iommu_system_reset"):
            if active or disk:
                raise ValueError(f"Reset avec droits actifs à {i}")
            route = False
        if s.startswith("virtio_iommu_switch_address_space Device 00:07.0"):
            if not s.endswith("(iommu enabled=1)"):
                raise ValueError(f"Route réseau imprévue à {i}")
            route = True
            counts["route_nic_active"] += 1
        elif s.startswith("pci_cfg_write virtio-net-pci"):
            if not route:
                raise ValueError(f"Accès PCI avant route démontrée à {i}")
            m = re.fullmatch(r"pci_cfg_write virtio-net-pci 00:07\.0 @(0x[\da-f]+) <- (0x[\da-f]+)", s)
            if not m or int(m[1], 16) < 4:
                raise ValueError(f"Écriture PCI ambiguë à {i}")
            if int(m[1], 16) == 4 and int(m[2], 16) & 4:
                bme.append(i)
        elif s.startswith("virtio_iommu_attach "):
            if disk or s != "virtio_iommu_attach domain=2 endpoint=48":
                raise ValueError(f"Attribution inattendue à {i}")
            disk = True
            counts["attache_disque"] += 1
        elif s.startswith("virtio_iommu_detach "):
            if active or not disk or s != "virtio_iommu_detach domain=2 endpoint=48":
                raise ValueError(f"Détachement non expliqué à {i}")
            disk = False
            counts["retrait_disque"] += 1
        elif s.startswith(("virtio_iommu_map ", "virtio_iommu_unmap_done ")):
            m = re.fullmatch(r"virtio_iommu_(map|unmap_done) domain=2 virt_start=(0x[\da-f]+) virt_end=(0x[\da-f]+)(?: phys_start=(0x[\da-f]+) flags=([123]))?", s)
            if not disk or not m:
                raise ValueError(f"Domaine ou format inattendu à {i}")
            lo, hi = int(m[2], 16), int(m[3], 16)
            if lo % 4096 or (hi + 1) % 4096 or hi < lo:
                raise ValueError(f"Étendue inattendue à {i}")
            if m[1] == "map":
                if not m[4] or int(m[4], 16) % 4096 or any(lo <= b and hi >= a for a,b in active):
                    raise ValueError(f"MAP non attribuable à {i}")
                active[lo, hi] = int(m[5])
            elif m[4] or active.pop((lo, hi), None) is None:
                raise ValueError(f"UNMAP sans MAP à {i}")
            counts[m[1]] += 1
            counts["pic_plages"] = max(counts["pic_plages"], len(active))
        elif s.startswith("virtio_iommu_translate "):
            m = re.fullmatch(r"virtio_iommu_translate mr=virtio-iommu-memory-region-2-48 rid=48 addr=(0x[\da-f]+) flag=([12])", s)
            if not disk or not m or not any(a <= int(m[1], 16) <= b and rights & int(m[2]) for (a,b), rights in active.items()):
                raise ValueError(f"Traduction hors octroi documenté à {i}")
            counts["traductions_disque"] += 1
        elif s.startswith(("virtio_iommu_report_fault", "virtio_iommu_set_config")):
            raise ValueError(f"Faute ou configuration imprévue à {i}")
    serial = (archive / "serie.log").read_bytes()
    if active or disk or not route or counts["attache_disque"] != 1 or counts["retrait_disque"] != 1 or \
       not counts["traductions_disque"] or b"MATERIEL bilan ok=true" not in serial or \
       b"actifs=0 BME=0 bypass=0" not in serial:
        raise ValueError("Handoff DXE incomplet")
    return {**counts, "ecritures_demandant_bme_nic_lignes": bme, "bail_nic": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    print(json.dumps(verifier(parser.parse_args().archive), indent=2, sort_keys=True))
