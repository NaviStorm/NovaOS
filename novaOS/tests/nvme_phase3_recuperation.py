"""Relire les deux générations des 16 RAW de coupure déjà qualifiés, sans démarrer de VM."""
import argparse
import json
from pathlib import Path
import subprocess

from nvme_phase2_oracle import FILE, NEW, OLD, P9_BLOCKS, fingerprint, inspect, view
from vtd_edu import require

POINTS = ("before-write", "during-write", "after-write", "during-f1", "after-f1",
          "during-publish", "during-f2", "after-f2")
RAPPORT = Path(__file__).resolve().parents[1] / "docs/validation/nvme-phase2-qemu.json"


def lire_slot(lecteur, donnees, secteur, slot):
    fs, lv = (s.hex() for s in (b"NOVA0190-Q35-P9F", b"NOVA0190-Q35-P9V"))
    return subprocess.run([lecteur, "salvage", str(P9_BLOCKS), str(secteur), fs, lv, str(slot), FILE],
                          input=donnees, capture_output=True, check=True).stdout


def controler(campagne, lecteur, matrice):
    require(fingerprint(lecteur.read_bytes()) == matrice["sha256_lecteur"], "Lecteur P9 inattendu")
    require(fingerprint((campagne / "bilan.json").read_bytes()) == matrice["sha256_bilan_complet"],
            "Matrice locale différente de la qualification de phase 2")
    resultats = []
    for r in matrice["cellules"]:
        point, secteur = r["cas"], r["secteur"]
        if point not in POINTS:
            continue
        dossier = campagne / f"{point}-{r['vcpus']}-{secteur}"
        raw = dossier / "disque.raw"
        require(fingerprint(raw.read_bytes()) == r["raw_sha256"], f"Image divergente {dossier}")
        etat = inspect(lecteur, raw, secteur)
        donnees = view(raw)
        anterieurs = [slot for slot, ancre in etat["ancres"].items() if ancre["generation"] == 3]
        require(len(anterieurs) == 1, f"Ancre confirmée de génération 3 absente ou ambiguë : {dossier}")
        confirme = anterieurs[0]
        courant = etat["slot"]
        require(etat["etat"] == r["etat"] and r["etat"] in ("ancien", "nouveau"), "État courant divergent")
        ancien = lire_slot(lecteur, donnees, secteur, confirme)
        actuel = lire_slot(lecteur, donnees, secteur, courant)
        require(ancien == OLD and actuel == (NEW if r["etat"] == "nouveau" else OLD),
                f"Contenu différent dans la génération confirmée ou courante de {dossier}")
        require(etat["generation"] == r["generation"] and not r["ack"]
                and fingerprint(actuel) == r["fichier_sha256"]
                and fingerprint(raw.read_bytes()) == r["raw_sha256"], "Génération, retour ou SHA incohérents")
        resultats.append(dict(point=point, secteur=secteur, slot_generation_3=confirme, slot_courant=courant,
                              generation_courante=etat["generation"], octets_ancien=len(ancien),
                              octets_actuel=len(actuel), sha256_ancien=fingerprint(ancien),
                              sha256_actuel=fingerprint(actuel), sha256_raw=r["raw_sha256"]))
    require(len(resultats) == 16 and {(r["point"], r["secteur"]) for r in resultats} ==
            {(p, s) for p in POINTS for s in (512, 4096)}, "Matrice incomplète ou doublon")
    return resultats


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campagne", type=Path)
    parser.add_argument("--lecteur", required=True, type=Path)
    args = parser.parse_args()
    releve = controler(args.campagne, args.lecteur.resolve(strict=True), json.loads(RAPPORT.read_bytes()))
    print(json.dumps(releve, ensure_ascii=False, indent=2))
