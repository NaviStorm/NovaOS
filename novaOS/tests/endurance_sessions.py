"""64 montages, publications et retraits de tous les services dans le même invité."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile


def empreinte(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def verifier(lecteur, disque, secteur, fs, lv, copies, source):
    def lire(op, *args):
        with disque.open("rb") as f:
            return subprocess.check_output([lecteur, op, "1024", str(secteur), *args],
                                           stdin=f, stderr=subprocess.PIPE)
    lire("verify", fs, lv)
    ancres = re.findall(r"slot=(\d+) génération=(\d+)".encode(), lire("inspect"))
    assert len(ancres) == 2
    slot = max(ancres, key=lambda paire: int(paire[1]))[0]
    assert lire("salvage", fs, lv, slot, "/source") == source
    assert lire("salvage", fs, lv, slot, "/occupe") == b"protege"
    for nom, attendu in copies:
        if lire("salvage", fs, lv, slot, "/" + nom) != attendu:
            raise ValueError(f"Contenu divergent après remontage de {nom}")
    return sorted(int(gen) for _, gen in ancres)


def qualifier(archive, arch, secteur, d):
    from cli import HOST, Terminal
    from vi import SOURCE, fixture
    disque, fs, lv = fixture(d, secteur)
    avant = verifier(HOST, disque, secteur, fs, lv, [], SOURCE)
    copies = []
    with Terminal(arch, secteur, disque, f"endurance-{secteur}", test=True) as t:
        def attendre(marque):
            return t.until(marque.encode(), 120, refus=b"BLOC session terminee ok=false")
        def envoyer(texte):
            t.vm.stdin.write(texte.encode())
        for numero in range(1, 65):
            attendre("nsh> ")
            envoyer("vi source\n")
            attendre("\x1b[6n")
            envoyer("\x1b[24;80R")
            attendre("source [enregistré] 1:1")
            nom = f"e{numero:02d}"
            envoyer(f"i{nom}\x1b:w occupe\r")
            attendre("remplacement refusé")
            envoyer(f":w {nom}\r")
            attendre("Saisir OUI puis Entrée : ")
            envoyer("OUI\r")
            attendre("fichier enregistré")
            envoyer(":q!\r")
            attendre("nsh> ")
            envoyer("exit\n")
            attendre(f"ENDURANCE tour={numero} ")
            copies.append((nom, nom.encode() + SOURCE))
        attendre("BLOC session terminee ok=true")
    serie = t.archive.read_text()
    comptes = [[int(n) for n in rang] for rang in re.findall(
        r"ENDURANCE tour=(\d+) libres=(\d+) blocs=(\d+) occupes=(\d+) engages=(\d+) processus=(\d+) pages_ipc=(\d+)", serie)]
    assert [r[0] for r in comptes] == list(range(1, 65)) and all(r[-2:] == [0, 0] for r in comptes)
    assert all(r[1:] == comptes[0][1:] for r in comptes)
    assert len(re.findall(r"BLOC bilan mode=42 (?:\w+=Exit\(0\) ){4}retrait=Ok\(\(\)\)", serie)) == 64
    apres = verifier(HOST, disque, secteur, fs, lv, copies, SOURCE)
    assert apres[-1] >= avant[-1] + len(copies)
    cible = "aarch64-unknown-none-softfloat" if arch == "aarch64" else "x86_64-unknown-none"
    return dict(arch=arch, secteur=secteur, sessions=len(comptes), cadres_libres=comptes[0][1],
                blocs_tas=comptes[0][2], octets_tas_occupes=comptes[0][3],
                octets_tas_engages=comptes[0][4], processus_fin=0, pages_ipc_fin=0,
                copies_relues=len(copies), refus_nom_occupe=len(copies), avant=avant, apres=apres,
                sha256_serie=empreinte(t.archive), sha256_image=empreinte(disque),
                sha256_elf=empreinte(archive / "novaOS/target" / cible / "release/novaos"),
                sha256_lecteur=empreinte(HOST))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("arch", choices=("aarch64", "x86_64"))
    parser.add_argument("secteur", type=int, choices=(512, 4096))
    parser.add_argument("rapport", type=Path)
    args = parser.parse_args()
    archive = args.archive.resolve()
    sys.path.insert(0, str(archive / "novaOS/tests"))
    with tempfile.TemporaryDirectory(prefix="novaos-endurance-") as tmp:
        resultat = qualifier(archive, args.arch, args.secteur, Path(tmp))
    with args.rapport.open("x") as f:
        json.dump(resultat, f, indent=2, ensure_ascii=False)
        f.write("\n")
