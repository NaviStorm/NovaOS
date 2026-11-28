"""Relire le dossier d’observations extrait, sans VM."""
from hashlib import sha256
from pathlib import Path
import json
import re
import sys

from extraire import EMPREINTE
from pixels import TEXTE, png, verifier

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "live-5z"))
from relire import nominal


def charger(p):
    return json.loads(p.read_bytes())


def exiger(test, erreur):
    if not test:
        raise ValueError(erreur)


def arret(registres, n):
    exiger(set(registres) == {str(i) for i in range(n)}, "Nombre de CPU arrêtés")
    for r in registres.values():
        exiger("HLT=1" in r and int(re.search(r"RFL=([0-9A-Fa-f]+)", r)[1], 16) & 0x200 == 0,
               "État terminal du CPU")


def capture(d, nom):
    p = (d / (nom + ".ppm")).read_bytes()
    exiger(png(p) == (d / (nom + ".png")).read_bytes(), "Conversion de la capture " + nom)
    return p


def relire(racine):
    for ram, n in ((256, 1), (384, 2), (512, 4)):
        d = racine / f"nominal-{ram}-{n}"
        print(nominal(d, EMPREINTE))
        seq = [1, 2, 4, 8 | n << 16, 16]
        textes = TEXTE.copy()
        textes[4] = f"4  SMP verifie - {n:02} CPU"
        for nom, nombre in (("entree", 0), ("memoire", 2), ("premiere-tache", 2),
                            ("ordonnanceur", 3), ("smp", 4), ("terminal", 5)):
            m = charger(d / (nom + ".json"))
            raw = capture(d, nom)
            exiger(sha256(raw).hexdigest() == m["sha256_ppm"] and m["evenements"] == seq[:nombre]
                   and m["journal"] == (1 << nombre) - 1, "Pixels ou journal d’étape")
            if nombre:
                exiger(verifier(raw, textes[:nombre+1]) == m["pixels"], "Panneau")
            if nom in ("entree", "memoire"):
                s = m["surface"]
                exiger(s["pages"] == s["pages_superviseur_rw"] and s["pages"] > 0, "Droits de surface")
                for pa in s["tables"]:
                    exiger((d / f"video-{nom}-{pa:x}.bin").stat().st_size == 4096, "Table vidéo")
                if nom == "entree":
                    avant = s
                else:
                    exiger(s["pages_nx"] == s["pages"] and s["cr3"] != avant["cr3"]
                           and s["surface_pa"] == avant["surface_pa"], "Transition CPU de la surface")
            if nom == "terminal":
                arret(m["registres_apres_reprise"], n)
                exiger(raw == capture(d, "terminal-stable"), "Stabilité du verdict")

    motifs = {1: "REFUS R01 - relais memoire", 2: "REFUS R02 - memoire noyau",
              3: "REFUS R03 - profil de boot", 4: "REFUS R04 - avant ordonnanceur"}
    for cas, code in (("relais", 1), ("memoire", 2), ("profil", 3), ("topologie", 4),
                      ("sans-surface", 6), ("sans-vga", 6)):
        d = racine / cas
        m = charger(d / "panne.json")
        exiger(m["image"] == EMPREINTE and m["refus"] == code and m["sata_fin"] == m["sata_efi"],
               "Refus ou stockage")
        seq = [256, 1, 2, 4, 65544] if code == 6 else [1]
        exiger(m["evenements"] == seq + [512 | code, 16], "Journal RAM du refus")
        arret(m["etats"], 1)
        if cas != "sans-vga":
            raw = capture(d, "refus")
            exiger(raw == capture(d, "refus-stable"), "Instabilité du refus")
            if code == 6:
                exiger(raw == capture(d, "avant"), "Écriture sans surface")
            else:
                verifier(raw, [*TEXTE[:2], "", "", "", motifs[code]])
    for role in ("LIMINE", "CONFIG", "MANIFESTE", "NOYAU"):
        m = charger(racine / "integrite" / role / "refus.json")
        exiger(m["motif"] == role and m["image"] != EMPREINTE and m["sata_efi"] == m["sata_fin"],
               "Refus du composant")
        arret(m["etats"], 1)
    print("3 parcours nominaux, 6 scénarios noyau et 4 refus du premier EFI vérifiés.")


if __name__ == "__main__":
    relire(Path(sys.argv[1]))
