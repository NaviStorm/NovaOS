"""Oracle du profil privé sans bail, commun aux deux invités QEMU."""
import re


def verifier_negociation(dossier, option, architecture):
    journal = (dossier / "serie.log").read_text().splitlines()
    lignes = [line for line in journal if line.startswith("NET EVENT_IDX ")]
    geometry = [line for line in journal if line.startswith(f"NET {architecture} installation file=")]
    expected = [f"NET {architecture} installation file={i} enable=1 statut=11 BME=0"
                for i in (0, 1)]
    if architecture not in ("Q35", "ARM") or option not in ("on", "off"):
        raise ValueError("Profil de négociation inconnu")
    cycles = {"Q35": 2, "ARM": 1}[architecture] if option == "on" else 0
    if geometry != expected * cycles:
        raise ValueError(f"Configuration effective des deux files incorrecte : {geometry}")
    requis = (1 << 33) | (1 << 32) | (1 << 29) | (1 << 5)
    if option == "off":
        m = re.fullmatch(r"NET EVENT_IDX refus=Feature offre=(0x[0-9a-f]+) statut=0 BME=0",
                         lignes[0]) if len(lignes) == 1 else None
        if not m or (int(m[1], 16) & requis) != (requis ^ (1 << 29)):
            raise ValueError(f"Refus natif non attribuable à EVENT_IDX : {lignes}")
        return {"offre": m[1], "refus": "Feature", "statut_apres_reset": 0}
    if option != "on" or len(lignes) != 2:
        raise ValueError(f"Deux négociations attendues : {lignes}")
    offres = []
    for line in lignes:
        m = re.fullmatch(r"NET EVENT_IDX offre=(0x[0-9a-f]+) relu=(0x[0-9a-f]+) statut=11 BME=0", line)
        if not m or int(m[1], 16) & requis != requis or int(m[2], 16) != requis:
            raise ValueError(f"Masque ou statut incompatible : {line}")
        offres.append(m[1])
    return {"offres": offres, "negocie": hex(requis), "statut": 11, "paires_de_files_preparees": cycles}
