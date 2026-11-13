"""Relecture de disques GPT/P9 jetables après publication ou coupure dans UTM."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import struct
import subprocess
import time
import uuid
import zlib

from es_groupees_elargies import GRANDS, LOCALISES, operation, verifier
from nsh_utm import demarrer, enregistrer, exporter, nommer, reouvrir_vi, transfert
from publication_conditionnelle import convertir
from esp_installation import construire
from verifier_gpt import verifier_gpt
from vi_ipc_sauvegarde import bilan

MIO = 1 << 20
LV = bytes([0x62])*16
IDS = (LV[:-1].hex()+"63", LV.hex())


def image(raw, volume, elf, s):
    esp = MIO//s
    p9 = esp * (61 if s == 512 else 265)
    fin = p9 + volume.stat().st_size//s
    dernier = fin + esp - 1
    n = 16384//s
    ids = uuid.uuid4().bytes_le + uuid.uuid4().bytes_le + LV
    table = bytearray(16384)
    types = ["c12a7328-f81f-11d2-ba4b-00a0c93ec93b", "3687f202-9e5a-4df6-83db-17bb308172f9"]
    for i, (nom, debut, stop) in enumerate([("EFI System", esp, p9), ("NovaFS P9", p9, fin)]):
        struct.pack_into("<16s16sQQQ72s", table, i*128, uuid.UUID(types[i]).bytes_le,
                         ids[16+i*16:32+i*16], debut, stop-1, 0, nom.encode("utf-16-le"))
    with raw.open("xb") as disk:
        disk.truncate((dernier+1)*s)
        def ecrire(lba, contenu):
            disk.seek(lba*s)
            disk.write(contenu)
        m = bytearray(s)
        cyl = dernier//(255*63)
        m[447:454] = bytes([0, 2, 0, 0xee, dernier//63%255, dernier%63+1 | (cyl>>2 & 192), cyl & 255])
        struct.pack_into("<II", m, 454, 1, dernier)
        m[510:512] = b"\x55\xaa"
        ecrire(0, m)
        for lba, autre, array in [(1, dernier, 2), (dernier, 1, dernier-n)]:
            entete = bytearray(struct.pack("<8sIIIIQQQQ16sQIII", b"EFI PART", 65536, 92, 0, 0,
                                          lba, autre, 2+n, dernier-n-1, ids[:16], array, 128, 128,
                                          zlib.crc32(table)))
            struct.pack_into("<I", entete, 16, zlib.crc32(entete))
            ecrire(lba, entete)
            ecrire(array, table)
        fat = construire(elf, s)
        for pos in range(0, len(fat), s+4):
            ecrire(esp+struct.unpack_from("<I", fat, pos)[0], fat[pos+4:pos+4+s])
        disk.seek(p9*s)
        with volume.open("rb") as part:
            shutil.copyfileobj(part, disk)
    verifier_gpt(raw, s, ids)
    return ids


def relire_systeme(disque, fmt, relu, s, ids, blocs, attendu, destination):
    convertir(disque, fmt, relu, "raw")
    table = verifier_gpt(relu, s, ids)
    extrait = relu.with_suffix(".p9")
    with relu.open("rb") as d, extrait.open("xb") as part:
        d.seek(table["p9"]*s)
        part.write(d.read((table["fin_p9"]-table["p9"]+1)*s))
    return verifier(extrait, s, blocs, attendu, IDS, destination=destination)


def relire_export(paquet, cible, s, ids, blocs, attendu, destination):
    return relire_systeme(paquet/"Data"/cible, Path(cible).suffix[1:], paquet.with_suffix(".raw"),
                          s, ids, blocs, attendu, destination)


def publier(c, nom, destination, coupure):
    os.write(c.fd, b":")
    c.attendre(f"vi [COMMANDE] {nom} [modifié]".encode())
    debut = time.monotonic_ns()
    os.write(c.fd, f"w {destination}\r".encode())
    c.attendre("Saisir OUI puis Entrée : ".encode())
    oui = time.monotonic_ns()
    os.write(c.fd, b"OUI\r")
    succes = "fichier enregistré"
    repere = "COUPURE groupe " + coupure if coupure else succes
    reponses = (succes, "erreur d’E/S", "publication incertaine", "capacité révoquée",
                "échec de l’opération", "destination non autorisée")
    marque = c.attendre_une(reponses + ((repere,) if coupure else ()), 180)
    if marque != repere:
        raise RuntimeError(f"Sauvegarde UTM de {nom} vers {destination} : {marque}")
    return debut, oui, time.monotonic_ns()


def essai(template, elf, generateur, s, out, sonde, index, cas, traces_bloc=True,
          format_image="qcow2", coupure=None, preallocation=False, repetition=1):
    out.mkdir()
    volume, raw = out/"volume.raw", out/"neuf.raw"
    etat = json.loads(subprocess.check_output([generateur, "--image", volume, str(s), "utm"]))
    repetitions = []
    destination = "coupure-" + coupure if coupure else "copie"
    destinations = [f"copie-{i}" for i in range(1, repetition + 1)] if repetition > 1 else [destination]
    destination = destinations[0]
    avant = verifier(volume, s, etat["blocs"], None, IDS, destination=destination)
    ids = image(raw, volume, elf, s)
    paquet = out/"prototype.utm"
    data = paquet/"Data"
    data.mkdir(parents=True)
    config = plistlib.loads((template/"config.plist").read_bytes())
    assert [d["ImageName"] for d in config["Drive"]] == ["code.fd", "variables.fd", "cible.qcow2"]
    for name in ("code.fd", "variables.fd"):
        shutil.copyfile(template/"Data"/name, data/name)
    args = config["QEMU"]["AdditionalArguments"]
    pos, = [i for i, v in enumerate(args) if v.startswith("virtio-blk-pci,")]
    args[pos] += f",logical_block_size={s},physical_block_size={s}"
    if not traces_bloc:
        # Témoin de mesure : conserver toutes les traces IOMMU du profil.
        traces = {"virtio_blk_handle_read", "virtio_blk_handle_write"}
        for event in traces:
            i = args.index(event)
            assert i > 0 and args[i - 1] == "-trace"
            del args[i - 1 : i + 1]
    cible = "cible." + format_image
    config["Drive"][2]["ImageName"] = cible
    (paquet/"config.plist").write_bytes(plistlib.dumps(config))
    if preallocation:
        subprocess.run(["qemu-img", "convert", "-f", "raw", "-O", "qcow2",
                        "-o", "preallocation=full", raw, data/cible], check=True)
    else:
        convertir(raw, "raw", data/cible, format_image)
    etat_image = json.loads(subprocess.check_output(["qemu-img", "info", "--output=json", data/cible]))
    paquet, ident = nommer(paquet, "cow")
    journal = out/"sauvegarde.serial"
    with enregistrer(paquet, ident) as machine:
        with demarrer(machine, journal) as c:
            transfert(c, b"nsh> ", 56)
            nom = "nouveau" if cas == "neuf-4m" else "source"
            for i, nom_copie in enumerate(destinations):
                if cas == "neuf-4m":
                    os.write(c.fd, b"vi nouveau\n")
                    c.attendre(b"\x1b[6n")
                    os.write(c.fd, b"\x1b[24;80R")
                    c.attendre(b"nouveau [nouveau] 1:1")
                else:
                    reouvrir_vi(c, nom)
                contenu = operation("index" if index else cas, lambda texte: os.write(c.fd, texte.encode()),
                                    lambda texte: c.attendre(texte.encode(), 180))
                depart, accord, acquit = publier(c, nom, nom_copie, coupure)
                if i == 0:
                    attendu, debut, oui, fin = contenu, depart, accord, acquit
                else:
                    assert contenu == attendu
                if repetition > 1:
                    repetitions.append(dict(destination=nom_copie, sauvegarde_ms=round((acquit-depart)/1e6, 3),
                                            apres_oui_ms=round((acquit-accord)/1e6, 3)))
                if not coupure:
                    os.write(c.fd, b":q\r")
                    c.attendre(b"nsh> ")
            if not coupure:
                c.ligne("exit", b"SYSTEME bilan Ok(())")
        if coupure:
            # Le moteur reste suspendu sur la frontière jusqu'au stop --force de demarrer.
            if coupure != "f2":
                attendu = None
            archive = out/"interrompu.utm"
            exporter(machine, archive)
            immediat = relire_export(archive, cible, s, ids, etat["blocs"], attendu, destination)
        with demarrer(machine, out/"reprise.serial") as c:
            transfert(c, b"nsh> ", 56)
            reouvrir_vi(c, destination if attendu is not None else "source")
            os.write(c.fd, b":q\r")
            c.attendre(b"nsh> ")
            c.ligne("exit", b"SYSTEME bilan Ok(())")
        exporter(machine, out/"relu.utm")
    apres = relire_export(out/"relu.utm", cible, s, ids, etat["blocs"], attendu, destination)
    for nom_copie in destinations[1:]:
        verifier(out/"relu.p9", s, etat["blocs"], attendu, IDS, destination=nom_copie)
    if coupure:
        return dict(frontiere=coupure, secteur=s, environnement=f"UTM {format_image.upper()}",
                    arret="utmctl stop --force", acquit_avant_coupure=False, octets_inseres=4*MIO,
                    avant=avant, juste_apres_coupure=immediat, apres_redemarrage=apres,
                    destination_complete=attendu is not None, source_et_destination_verifiees=True)
    mesure = out/"mesure.json"
    mesure.write_text(json.dumps([dict(octets=4*MIO, transfert_avant_oui_ms=(oui-debut)/1e6,
                                      publication_apres_oui_ms=(fin-oui)/1e6)]))
    trace = journal.read_text()
    if sonde:
        compteur = bilan(journal, mesure)
    else:
        assert "SAVE from=" not in trace
        compteur = None
    pic = int(re.search(r"BLOC memoire [^\n]*pic_cadres=(\d+)", trace)[1])
    return dict(cas=cas, index_parcouru=index, sonde=sonde, traces_bloc=traces_bloc, preallocation=preallocation,
                repetitions=repetitions,
                image_initiale={k: etat_image[k] for k in ("format", "virtual-size", "actual-size", "cluster-size")
                                if k in etat_image}, pic_cadres_session=pic, sauvegarde_ms=round((fin-debut)/1e6, 3),
                apres_oui_ms=round((fin-oui)/1e6, 3), secteur=s, environnement=f"UTM {format_image.upper()}", avant=avant, apres=apres, redemarrages=1,
                octets_relus=True, compteur=compteur)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for champ in ("profil_systeme", "elf_systeme", "generateur", "sortie"):
        p.add_argument(champ, type=Path)
    p.add_argument("secteur", type=int, choices=[512, 4096])
    p.add_argument("--format", choices=("raw", "qcow2"), default="qcow2", help="format du disque jetable")
    p.add_argument("--preallocation", action="store_true", help="préallouer complètement le QCOW2 jetable avant UTM")
    p.add_argument("--repetition", type=int, choices=(1, 2), default=1,
                   help="deux copies localisées vers des noms distincts sur le même support")
    p.add_argument("--sans-sonde", action="store_true")
    p.add_argument("--sans-traces-bloc", action="store_true",
                   help="mesure témoin sans trace QEMU par lecture/écriture, avec les traces IOMMU")
    p.add_argument("--index", action="store_true", help="rejouer le parcours G/gg de la référence")
    p.add_argument("--coupure", choices=("pages", "f1", "f2"),
                   help="arrêt forcé au point du moteur, avec --cas ajout-4m et les fixtures de coupure")
    p.add_argument("--cas", choices=("debut", "milieu", "fin", "multiple", *LOCALISES, *GRANDS), default="debut")
    a = p.parse_args()
    if a.index and a.cas != "debut":
        p.error("--index accompagne le cas debut")
    if a.preallocation and a.format != "qcow2":
        p.error("--preallocation requiert QCOW2")
    if a.repetition > 1 and (a.coupure or not a.sans_sonde or a.cas not in LOCALISES or a.index):
        p.error("--repetition requiert une édition localisée hors sonde")
    if a.coupure and (a.cas != "ajout-4m" or not a.sans_sonde):
        p.error("--coupure requiert --cas ajout-4m et --sans-sonde")
    # Un seul banc UTM par campagne pour mesurer le profil à charge maîtrisée.
    with (a.sortie.resolve().parent / ".mesure-utm.lock").open("a") as verrou:
        fcntl.flock(verrou, fcntl.LOCK_EX)
        r = essai(a.profil_systeme, a.elf_systeme, a.generateur.resolve(), a.secteur,
                  a.sortie.resolve(), not a.sans_sonde, a.index, a.cas, not a.sans_traces_bloc,
                  a.format, a.coupure, a.preallocation, a.repetition)
    (a.sortie/"resultats.json").write_text(json.dumps(r, ensure_ascii=False, indent=2)+"\n")
