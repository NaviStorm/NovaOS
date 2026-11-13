"""Application UTM, GPT et P9 sur un QCOW2 neuf ; deux démarrages du système."""
import argparse
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

from es_groupees_elargies import operation, verifier
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


def essai(template, elf, generateur, s, out, sonde, index, cas):
    out.mkdir()
    volume, raw = out/"volume.raw", out/"neuf.raw"
    etat = json.loads(subprocess.check_output([generateur, "--image", volume, str(s), "utm"]))
    avant = verifier(volume, s, etat["blocs"], None, IDS)
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
    (paquet/"config.plist").write_bytes(plistlib.dumps(config))
    convertir(raw, "raw", data/"cible.qcow2", "qcow2")
    paquet, ident = nommer(paquet, "cow")
    journal = out/"sauvegarde.serial"
    with enregistrer(paquet, ident) as machine:
        with demarrer(machine, journal) as c:
            transfert(c, b"nsh> ", 56)
            reouvrir_vi(c, "source")
            attendu = operation("index" if index else cas, lambda texte: os.write(c.fd, texte.encode()),
                               lambda texte: c.attendre(texte.encode(), 180))
            os.write(c.fd, b":")
            c.attendre("vi [COMMANDE] source [modifié]".encode())
            debut = time.monotonic_ns()
            os.write(c.fd, b"w copie\r")
            c.attendre("Saisir OUI puis Entrée : ".encode())
            oui = time.monotonic_ns()
            os.write(c.fd, b"OUI\r")
            c.attendre("fichier enregistré".encode())
            fin = time.monotonic_ns()
            os.write(c.fd, b":q\r")
            c.attendre(b"nsh> ")
            c.ligne("exit", b"SYSTEME bilan Ok(())")
        with demarrer(machine, out/"reprise.serial") as c:
            transfert(c, b"nsh> ", 56)
            reouvrir_vi(c, "copie")
            os.write(c.fd, b":q\r")
            c.attendre(b"nsh> ")
            c.ligne("exit", b"SYSTEME bilan Ok(())")
        exporter(machine, out/"relu.utm")
    relu = out/"relu.raw"
    convertir(out/"relu.utm/Data/cible.qcow2", "qcow2", relu, "raw")
    table = verifier_gpt(relu, s, ids)
    with relu.open("rb") as d, (out/"extrait.raw").open("xb") as part:
        d.seek(table["p9"]*s)
        part.write(d.read((table["fin_p9"]-table["p9"]+1)*s))
    apres = verifier(out/"extrait.raw", s, etat["blocs"], attendu, IDS)
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
    return dict(cas=cas, index_parcouru=index, sonde=sonde, pic_cadres_session=pic, sauvegarde_ms=round((fin-debut)/1e6, 3),
                apres_oui_ms=round((fin-oui)/1e6, 3), secteur=s, environnement="UTM QCOW2", avant=avant, apres=apres, redemarrages=1,
                octets_relus=True, compteur=compteur)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for champ in ("profil_systeme", "elf_systeme", "generateur", "sortie"):
        p.add_argument(champ, type=Path)
    p.add_argument("secteur", type=int, choices=[512, 4096])
    p.add_argument("--sans-sonde", action="store_true")
    p.add_argument("--index", action="store_true", help="rejouer le parcours G/gg de la référence")
    p.add_argument("--cas", choices=("debut", "milieu", "fin", "multiple"), default="debut")
    a = p.parse_args()
    if a.index and a.cas != "debut":
        p.error("--index accompagne le cas debut")
    r = essai(a.profil_systeme, a.elf_systeme, a.generateur.resolve(), a.secteur, a.sortie.resolve(), not a.sans_sonde, a.index, a.cas)
    (a.sortie/"resultats.json").write_text(json.dumps(r, ensure_ascii=False, indent=2)+"\n")
