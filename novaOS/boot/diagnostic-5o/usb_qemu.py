"""Démarrage USB réel du modèle xHCI, source ordinaire en lecture seule."""

import argparse
from contextlib import nullcontext
import json
from pathlib import Path
import sys
from image5o import ROOT, MIO, chargeur, lecteur, verifier
from demarrage import boot, STORES, public_member

sys.path.insert(0, str(ROOT / "tests/ethernet_v1"))
from port_qemu import qualification

sys.path.insert(0, str(ROOT / "boot/diagnostic-5m"))
from supports import comparer_magasin, pe_info


def campagne(folder, fw, images, mag, cas_retenus=("unique", "ambigu", "hors-db")):
    folder.mkdir(mode=0o700)
    pe = chargeur()
    original = folder / "original.fd"
    original.write_bytes(public_member("actif.fd", STORES["actif"]))
    comparer_magasin(original, mag, [pe_info(pe)["authenticode"]])
    rows = {}
    with qualification(ROOT.parent.parent, {}):
        for cas in cas_retenus:
            for mode in ("actif",) if cas == "hors-db" else ("actif", "inactif"):
                store = (
                    mag.read_bytes()
                    if mode == "actif" and cas != "hors-db"
                    else public_member(mode + ".fd", STORES[mode])
                )
                cal = None
                for m in (68, 1024, 8192, 16384) if cas == "unique" else (68,):
                    p = images / f"{m}-media.img"
                    name = f"{cas}-{mode}-{m}"
                    with (
                        lecteur(p) as f,
                        (
                            lecteur(images / "68-bis.img")
                            if cas == "ambigu"
                            else nullcontext(None)
                        ) as clone,
                    ):
                        verifier(f, m * MIO, pe)
                        options = dict(
                            image_test=pe,
                            variables_test=store,
                            duplicate=clone,
                            ecran=True,
                        )
                        if cas != "hors-db" and cal is None:
                            cal = boot(
                                folder / ("cal-" + name), fw, mode, f, **options
                            )["base"]
                        result = boot(
                            folder / name,
                            fw,
                            mode,
                            f,
                            cal,
                            temoin=cas == "hors-db",
                            **options,
                        )
                        verifier(f, m * MIO, pe)
                        result["ecran_immobile_du_premier_rip_au_hlt"] = (
                            True if cas != "hors-db" else None
                        )
                        result["export_autorise"] = False
                        rows[name] = result
                        (folder / "bilan.json").write_text(
                            json.dumps(rows, indent=2) + "\n"
                        )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for n in ("dossier", "firmware", "images", "magasin"):
        p.add_argument(n, type=Path)
    p.add_argument("--cas", choices=("unique", "ambigu", "hors-db"), action="append")
    a = p.parse_args()
    campagne(
        a.dossier.resolve(),
        a.firmware.resolve(),
        a.images.resolve(),
        a.magasin.resolve(),
        a.cas or ("unique", "ambigu", "hors-db"),
    )
