"""Relecture indépendante des traductions et attributions du banc DXE de pré-attribution."""
if not __debug__:
    raise RuntimeError("Les contrôles de qualification exigent les assertions Python.")

import argparse
from collections import Counter
import json
from pathlib import Path
import re


def verifier(out, candidat):
    counts = Counter()
    routes = set()
    domaines = {}
    ranges = {1: {}, 2: {}, 3: {}}
    top = {1: 48, 2: 48, 3: 56}
    fin_dxe = False
    for phase in ["reset", "fin"]:
        state = json.loads((out / (phase + ".json")).read_text())
        if state["vm"]["running"] or state["boot_bypass"] is not False or state["iommu_platform"] is not True or \
                state.get("pci_id_ecam") != 0x10411af4 or state["pci_command"] & 4:
            raise ValueError(f"Capture {phase} incohérente")
    with (out / "qemu.trace").open() as stream:
        for i, line in enumerate(stream, 1):
            line = line.rstrip()
            def exiger(ok):
                if not ok:
                    raise ValueError(f"Trace IOMMU incohérente à la ligne {i}: {line}")
            if line.startswith("virtio_iommu_system_reset"):
                exiger(not domaines)
                routes.clear()
            elif line.startswith("virtio_iommu_switch_address_space "):
                m = re.fullmatch(r".* Device 00:([0-9a-f]{2})\.0 switching address space \(iommu enabled=1\)", line)
                exiger(m)
                routes.add(int(m[1], 16) * 8)
            elif line.startswith("pci_cfg_write virtio-net-pci"):
                m = re.fullmatch(r".* 00:07\.0 @(0x[0-9a-f]+) <- (0x[0-9a-f]+)", line)
                exiger(m and 56 in routes and int(m[1], 16) >= 4)
                if int(m[1], 16) == 4 and int(m[2], 16) & 4:
                    exiger(not fin_dxe and 56 not in domaines)
                    counts["bme_firmware_sans_domaine"] += 1
            elif line.startswith(("virtio_iommu_attach ", "virtio_iommu_detach ")):
                m = re.fullmatch(r"virtio_iommu_(attach|detach) domain=(\d+) endpoint=(\d+)", line)
                exiger(m)
                d, sid = int(m[2]), int(m[3])
                exiger(top.get(d) == sid and sid in routes and not ranges[d])
                if m[1] == "attach":
                    exiger(sid not in domaines and (d == 2) != fin_dxe)
                    domaines[sid] = d
                else:
                    exiger(domaines.pop(sid, None) == d)
                    fin_dxe |= d == 2
                counts[f"{m[1]}_{d}"] += 1
            elif line.startswith(("virtio_iommu_map ", "virtio_iommu_unmap_done ")):
                m = re.fullmatch(r"virtio_iommu_(map|unmap_done) domain=(\d+) virt_start=(0x\w+) virt_end=(0x\w+)(?: phys_start=(0x\w+) flags=([123]))?", line)
                exiger(m)
                d, lo, hi = int(m[2]), int(m[3], 16), int(m[4], 16)
                exiger(d in domaines.values() and lo % 4096 == 0 and hi >= lo and (hi+1) % 4096 == 0)
                if m[1] == "map":
                    pa, perm = int(m[5], 16), int(m[6])
                    exiger(pa % 4096 == 0 and not any(lo <= b and hi >= a for a, b in ranges[d]))
                    if d in (1, 3):
                        base = 0x8000000 if d == 1 else 0x10000000
                        slot = (lo-base)//4096
                        droits = (1, 1, 3, 1, 2, 2, 1) if d == 1 else \
                            (1, 1, 2, 1, 1, 2, *([2]*8), *([1]*8))
                        exiger(0 <= slot < len(droits) and hi == lo+4095 and perm == droits[slot])
                        exiger(not any(p == pa for batch in ranges.values() for p, _ in batch.values()))
                    ranges[d][lo, hi] = pa, perm
                    counts[f"pic_pages_{d}"] = max(counts[f"pic_pages_{d}"], len(ranges[d]))
                else:
                    exiger(m[5] is None and ranges[d].pop((lo, hi), None) is not None)
                counts[f"{m[1]}_{d}"] += 1
            elif line.startswith("virtio_iommu_translate "):
                m = re.fullmatch(r".*memory-region-\d+-(\d+) rid=(\d+) addr=(0x\w+) flag=([12])", line)
                exiger(m and m[1] == m[2])
                sid, adresse, sens = int(m[2]), int(m[3], 16), int(m[4])
                d = domaines.get(sid)
                exiger(d in (1, 2) and any(a <= adresse <= b and perm & sens
                                           for (a, b), (_, perm) in ranges[d].items()))
                counts[f"traductions_{d}"] += 1
            elif line.startswith(("virtio_iommu_report_fault", "virtio_iommu_set_config")):
                exiger(False)
    if domaines or any(ranges.values()) or not fin_dxe or not counts["traductions_1"] or not counts["traductions_2"]:
        raise ValueError("Suivi incomplet des traductions ou restitutions")
    if candidat and counts["pic_pages_3"] != 22 or not candidat and counts["attach_3"]:
        raise ValueError("Domaines incompatibles avec le candidat annoncé")
    return dict(counts)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("dossier", type=Path)
    p.add_argument("--partage", action="store_true")
    a = p.parse_args()
    print(json.dumps(verifier(a.dossier, a.partage), sort_keys=True, indent=2))
