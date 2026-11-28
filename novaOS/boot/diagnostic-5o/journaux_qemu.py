"""Lecture seule par xHCI d'un média de laboratoire portant des journaux préparés hors VM."""

import argparse
import json
from pathlib import Path
import sys
from image5o import ROOT, LOG, MIO, chargeur, creer, lecteur, sha, volume, require
from demarrage import boot
from journaux import history, pad, sums

sys.path.insert(0, str(ROOT / "tests/ethernet_v1"))
from port_qemu import qualification


def exercer(folder, fw, mag):
    folder.mkdir(mode=0o700)
    p = folder / "rapports.img"
    pe = chargeur()
    creer(p, 68 * MIO, pe)
    _, areas = volume(LOG, "NOVALOGS")
    data, text, seal = areas[0]
    a = pad(b'{"schema":"temoin","diagnostic":"synthetique"}', data[1])
    b = pad(b"Rapport synthetique anterieur", text[1])
    # Le banc prépare uniquement son fichier neuf. Le second emplacement simule une coupure.
    with p.open("r+b") as f:
        for (pos, n), payload in (
            (data, a),
            (text, b),
            (seal, pad(sums(a, b), seal[1])),
            (areas[1][0], b"incomplet"),
        ):
            require(len(payload) <= n, "Fixture hors emplacement")
            f.seek(pos)
            f.write(payload)
    with qualification(ROOT.parent.parent, {}), lecteur(p) as f:
        before = history(f, areas)
        require(
            [x[0] for x in before] == ["valide", "interrompu"] + ["libre"] * 6,
            "Fixture",
        )
        origin = sha(f)
        # Revalider le magasin autorisé avec la même règle que la campagne USB.
        from demarrage import STORES, public_member

        sys.path.insert(0, str(ROOT / "boot/diagnostic-5m"))
        from supports import comparer_magasin, pe_info

        orig = folder / "original.fd"
        orig.write_bytes(public_member("actif.fd", STORES["actif"]))
        comparer_magasin(orig, mag, [pe_info(pe)["authenticode"]])
        opts = dict(image_test=pe, variables_test=mag.read_bytes(), ecran=True)
        base = boot(folder / "cal", fw, "actif", f, **opts)["base"]
        rec = boot(folder / "lecture", fw, "actif", f, base, **opts)
        after = history(f, areas)
        require(after == before and sha(f) == origin, "Journal ou média modifié")
    (folder / "bilan.json").write_text(
        json.dumps(
            dict(avant=before, apres=after, sha256_media=origin, demarrage=rec),
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for n in ("sortie", "firmware", "magasin"):
        p.add_argument(n, type=Path)
    a = p.parse_args()
    exercer(a.sortie.resolve(), a.firmware.resolve(), a.magasin.resolve())
