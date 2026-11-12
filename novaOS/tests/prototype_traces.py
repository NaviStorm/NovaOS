"""Recouper la série invitée, les opérations bloc et les traductions du moteur UTM."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re


if not __debug__:
    raise RuntimeError("La qualification exige les assertions Python actives")


def mesures(dossier, nom):
    serie = (dossier / (nom + ".serial")).read_bytes()
    trace = (dossier / (nom + ".trace")).read_bytes()
    texte = serie.decode(errors="replace")
    sid = 56 if nom == "autre-slot" else 48
    actif = None
    retrait = None
    plages = {}
    physiques = set()
    anciennes = set()
    droits = Counter()
    stats = {d: Counter() for d in (1, 2)}
    dernier = 0
    cycles = []
    lectures = []
    irq = Counter()
    fautes = refus = reutilisations = 0
    for ligne in trace.decode().splitlines():
        if m := re.fullmatch(r"virtio_iommu_(attach|detach) domain=(\d+) endpoint=(\d+)", ligne):
            op, d, endpoint = m[1], int(m[2]), int(m[3])
            assert endpoint == sid and not plages and not physiques and retrait is None, ligne
            if op == "attach":
                assert actif is None and d in stats, ligne
                actif, dernier = d, 0
                if d == 2:
                    anciennes.clear()
                cycles.append(d)
            else:
                assert actif == d, ligne
                actif = None
            stats[d][op] += 1
        elif m := re.fullmatch(r"virtio_iommu_map domain=(\d+) virt_start=(\w+) virt_end=(\w+) phys_start=(\w+) flags=(\d+)", ligne):
            d = int(m[1])
            lo, hi, pa, flags = (int(x, 0) for x in m.groups()[1:])
            assert actif == d and flags in (1, 2, 3) and hi >= lo and not (lo | (hi+1) | pa) & 4095, ligne
            assert 0x40000000 <= pa and pa + hi - lo < 0x50000000, ligne
            assert all(hi < start or end < lo for start, (end, _, _) in plages.items()), ligne
            pages = set(range(pa, pa + hi-lo+1, 4096))
            assert physiques.isdisjoint(pages), ligne
            if d == 2:
                assert 0x20000000 <= lo <= hi < 0xf0000000 and lo >= dernier, ligne
                dernier = hi + 1
                reutilisations += len(anciennes & pages)
                anciennes.update(pages)
                droits[flags] += 1
                if flags == 3:
                    # Une seule page mixte, la partie used de l’anneau privé.
                    assert hi - lo == 4095 and not any(r == 3 for _, _, r in plages.values()), ligne
            plages[lo] = (hi, pa, flags)
            physiques.update(pages)
            stats[d]["map"] += 1
        elif m := re.fullmatch(r"virtio_iommu_unmap domain=(\d+) virt_start=(\w+) virt_end=(\w+)", ligne):
            d, lo, hi = (int(x, 0) for x in m.groups())
            assert actif == d and lo in plages, ligne
            assert retrait is None, ligne
            retrait = (d, lo, hi)
        elif m := re.fullmatch(r"virtio_iommu_unmap_done domain=(\d+) virt_start=(\w+) virt_end=(\w+)", ligne):
            d, lo, hi = (int(x, 0) for x in m.groups())
            assert (d, lo, hi) == retrait, ligne
            retrait = None
            end, pa, _ = plages.pop(lo)
            assert end == hi, ligne
            physiques.difference_update(range(pa, pa+hi-lo+1, 4096))
            stats[d]["unmap"] += 1
        elif m := re.fullmatch(r"virtio_iommu_translate mr=\S+ rid=(\d+) addr=(\w+) flag=(\d+)", ligne):
            endpoint, adresse, sens = (int(x, 0) for x in m.groups())
            assert actif is not None and endpoint == sid and sens in (1, 2, 3), ligne
            permis = any(lo <= adresse <= hi and flags & sens == sens for lo, (hi, _, flags) in plages.items())
            if not permis:
                assert nom == "protection" and actif == 1 and 0x8004000 <= adresse < 0x8004200, ligne
                refus += 1
            stats[actif]["traductions"] += 1
        elif ligne.startswith("virtio_iommu_report_fault "):
            assert nom == "protection" and actif == 1 and f"endpoint={sid} " in ligne, ligne
            fautes += 1
        elif ligne.startswith("virtio_blk_handle_read "):
            assert actif is not None and plages and stats[actif]["traductions"], ligne
            stats[actif]["lectures"] += 1
            if actif == 1:
                assert ligne.endswith(" sector 16 nsectors 1"), ligne
                lectures.append(16)
        elif ligne.startswith("virtio_blk_handle_write "):
            raise AssertionError(ligne)
        elif m := re.match(r"gicv3_icc_(iar1_read|eoir_write).* value (0x\w+)", ligne):
            irq[m[1], int(m[2], 16)] += 1
        elif ligne.startswith("virtio_iommu_") or "is not known" in ligne:
            raise AssertionError(f"Événement IOMMU inattendu : {ligne}")

    assert retrait is None
    assert stats[2]["attach"] > 0 and stats[2]["lectures"] > 0 and droits[1] and droits[2] and droits[3]
    assert texte.count("politique active avant PCI") == stats[2]["attach"]
    assert texte.count(f"PROBE sid={sid} reserve=0 granule=4096") == stats[2]["attach"]
    assert texte.count(f"ATTACH sid={sid} domaine=2 bypass=0 BME=0") == stats[2]["attach"]
    sorties = re.findall(r"NOVA DXE SORTIE droits=(\d+) retraits=(\d+) actifs=0 BME=0 bypass=0", texte)
    assert all(a == b for a, b in sorties)
    if nom in ("coupure", "retrait-suspendu"):
        assert not sorties and actif == 2 and stats[2]["unmap"] == 0 and not stats[1] and plages
        assert "OBS EFI" not in texte and "NovaOS v" not in texte
        if nom == "coupure":
            assert json.loads((dossier / "coupure.json").read_text()) == {
                "lecture_en_vol": True, "lectures_finies": 0, "ecritures_finies": 0}
            assert stats[2]["lectures"] == 1
        else:
            assert "arret BME=0" in texte and "QUARANTAINE délai commande IOMMU" in texte
    else:
        assert actif is None and not plages and all(x["map"] == x["unmap"] for x in stats.values())
        assert len(sorties) == stats[2]["detach"] == stats[2]["attach"]
        # Un jeton commun de l’anneau produit deux commandes MAP.
        assert sum(int(a) for a, _ in sorties) + droits[3] == stats[2]["map"]
        assert reutilisations > 0
        if nom == "mapkey":
            assert cycles == [2] and texte.count("OBS EFI MapKey refusee") == texte.count("seconde sortie reussie") == 1
        elif nom == "autre-slot":
            assert cycles == [2] and "DMA bilan Err(Absent)" in texte
        else:
            boots = 2 if nom == "redemarrage" else 1
            assert cycles == ([2, 1, 1, 1, 1, 1] if nom == "protection" else [2, 1] * boots), cycles
            assert len(sorties) == boots == texte.count("DMA bilan Ok(())")
            sequence = (r"NOVA DXE politique active avant PCI.*?NOVA DXE ATTACH.*?premier MAP acquitte, BME=1"
                        r".*?OBS EFI StartImage Limine.*?NOVA DXE SORTIE.*?OBS EFI notification ExitBootServices"
                        r".*?MMU : sections et alias verifies.*?NovaOS v0[.]15[.]0.*?DMA bilan Ok")
            assert len(re.findall(sequence, texte, re.S)) == boots
            priorites = re.findall(r"GICv3 groupe=1 PMR=(0x\w+) PPI30=0x80 SPI37=0x40", texte)
            assert len(priorites) == boots and all(int(p, 16) > 0x80 for p in priorites)
            assert lectures and irq["iar1_read", 37] == irq["eoir_write", 37] >= boots
            if nom == "protection":
                assert refus and fautes and texte.count("temoin=true") == 2
                assert "DMA IRQ tardive acquittee sans livraison" in texte
            else:
                assert len(lectures) == boots
                assert texte.count("DMA motif correct IRQ=1 octets=512") == boots
            if nom == "cd":
                assert "droit refuse RID=8" in texte and "Pci(0x6,0x0)" in texte
    cadres = set()
    if nom == "reemploi":
        for a, b in re.findall(r"DMA pages reutilisees (0x\w+) (0x\w+)", texte):
            cadres.update(range(int(a, 16), int(b, 16), 4096))
        assert cadres & anciennes, "Aucun cadre ancien réellement réalloué"
    return dict(sid=sid, domaines=cycles, firmware=dict(stats[2]), noyau=dict(stats[1]),
                jetons_retires=[int(b) for _, b in sorties], lectures_lba16=len(lectures),
                pages_efi_distinctes=len(anciennes), reemplois_pages_dxe=reutilisations,
                pages_efi_reallouees_par_noyau=len(cadres & anciennes),
                droits_map={str(k): v for k, v in sorted(droits.items())}, plages_actives_finales=len(plages),
                eoi_ppi30=irq["eoir_write", 30], eoi_intx37=irq["eoir_write", 37],
                refus_de_traduction=refus, fautes=fautes, serie_sha256=hashlib.sha256(serie).hexdigest(),
                trace_sha256=hashlib.sha256(trace).hexdigest())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dossier", type=Path)
    args = parser.parse_args()
    reference = json.loads((args.dossier / "resultats.json").read_text())["essais"]
    traces = {p.stem for p in args.dossier.glob("*.trace")}
    assert traces and traces == set(reference), "Profils incomplets"
    for nom in sorted(traces):
        nouveau = mesures(args.dossier, nom)
        assert nouveau == reference[nom], f"Écart avec les mesures ou les empreintes du profil {nom}"
        print(nom, json.dumps(nouveau, ensure_ascii=False))
