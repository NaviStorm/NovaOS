"""Contre-lecture hors VM des témoins GDB, QMP et série conservés."""
import argparse
import json
from pathlib import Path
import re

from image import require


def nominal(dossier, empreinte):
    m = json.loads((dossier / "mesures.json").read_text())
    t = m["etats"]
    ordre = ("reset", "efi", "exit_boot_services", "retour_ebs", "nova_start", "apres_memoire",
             "tache_principale", "rendezvous")
    brut = [json.loads(l) for l in (dossier / "compteurs-qmp.jsonl").read_text().splitlines()]
    require(len(brut) == len(ordre), "Échantillons QMP manquants")
    for nom, noeuds in zip(ordre, brut):
        stats = [x["stats"] for x in noeuds if x.get("device") == "interne"]
        attendu = t[nom] if nom == "reset" else t[nom]["sata"]
        require(len(stats) == 1 and all(stats[0][k] == v for k, v in attendu.items()),
                "Désaccord des compteurs bruts et du relevé")
        if nom != "reset":
            require(attendu == t["efi"]["sata"], "E/S interne durant la chaîne")
    require(m["sha256_image"] == empreinte and m["statut_ebs"] == 0, "Image ou sortie UEFI")
    syms = m["symboles"]
    for etape, rip in (("efi", m["entree_efi"]), ("exit_boot_services", m["ebs"]),
                       ("retour_ebs", m["retour_ebs"]), ("nova_start", syms["nova_start"]), ("apres_memoire", syms["novaos::sched::start"]),
                       ("tache_principale", syms["novaos::main_thread"])):
        lu = int(re.search(r"RIP=([a-fA-F0-9]+)", t[etape]["registres"])[1], 16)
        require(lu == rip, "Point d’exécution " + etape)
    require(int(re.search(r"RAX=([a-fA-F0-9]+)", t["retour_ebs"]["registres"])[1], 16) == 0,
            "ExitBootServices n’a pas rendu EFI_SUCCESS")
    pages = m["pages"]
    require(pages["wp"] and pages["nxe"] and
            sum(pages["groupes"].values()) * 4096 == syms["__image_end"] - syms["__image_start"],
            "Couverture de la protection de l’image")
    for table in pages["tables"]:
        require((dossier / f"pt-{table:x}.bin").stat().st_size == 4096, "Capture de table CPU")
    trace = (dossier / "transactions.log").read_text().splitlines()
    for l in trace:
        msg = re.fullmatch(r"pci_cfg_write qemu-xhci ([0-9a-f:.]+) @0x4 <- (0x[0-9a-f]+)", l)
        require(msg is not None, "Transaction matérielle hors profil")
        dev = t["efi"]["pci"]["0000:" + msg[1]]
        require(dev["classe_revision"] >> 8 == 0x0c0330 and int(msg[2], 16) == dev["cmd"],
                "Changement de commande PCI USB")
    serie = (dossier / "serie.log").read_bytes()
    n = m["cpus"]
    for signal in (b"NOVA5Z USB verifie", b"NovaOS v0.20.0", b"SMP memoire",
                   f"SMP rendezvous={((1 << n) - 1):#x} actifs={n} OK".encode(),
                   b"SMP arret motif=normal"):
        require(signal in serie, "Série incomplète")
    return dict(mio=m["ram_mio"], cpu=n, lectures_firmware=t["efi"]["sata"]["rd_operations"],
                octets_firmware=t["efi"]["sata"]["rd_bytes"], io_interne_apres_efi=0,
                pages=pages["groupes"], descripteurs_limine=len(m["carte_limine"]))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("dossier", type=Path)
    p.add_argument("--sha256", required=True)
    a = p.parse_args()
    print(json.dumps(nominal(a.dossier, a.sha256), ensure_ascii=False, sort_keys=True))
