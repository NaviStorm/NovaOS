"""Sceller une campagne terminée. Les ELF de démarrage restent dans l’archive du laboratoire."""
import argparse
from collections import Counter
import gzip
import hashlib
import json
from pathlib import Path
import struct
import tarfile

from bilan import PERF, exiger, main as bilan
from qualifier import sha, write


def ethernet(path):
    """Empreinte des octets Ethernet longueur puis contenu, dans l’ordre du PCAP, hors temps."""
    digest = hashlib.sha256()
    comptes = Counter()
    longueurs = Counter()
    entetes = {
        bytes.fromhex("024e5600000152540012345688b5"): "tx",
        bytes.fromhex("525400123456024e5600000188b5"): "rx",
    }
    with gzip.open(path, "rb") as f:
        exiger(f.read(24) == struct.pack("<IHHIIII", 0xa1b2c3d4, 2, 4, 0, 0, 65535, 1), "PCAP Ethernet attendu")
        while h := f.read(16):
            _, _, n, original = struct.unpack("<IIII", h)
            exiger(n == original and n in (60, 1514), "longueur de capture invalide")
            trame = f.read(n)
            exiger(len(trame) == n and trame[:14] in entetes, "trame ou direction inattendue")
            sens = entetes[trame[:14]]
            comptes[sens] += 1
            longueurs[sens] += n
            digest.update(struct.pack(">I", n))
            digest.update(trame)
    return dict(trames=dict(comptes), octets=dict(longueurs), sha256_sans_temps=digest.hexdigest())


def capsule(archive, sortie):
    resultat = bilan(archive)
    sortie.mkdir(parents=True, exist_ok=True)
    write(sortie / "bilan.json", resultat)
    membres = []
    inventaire = {}
    captures = {}
    for cas, series in resultat["profils"].items():
        for serie in series:
            i = serie["demarrage"]
            d = archive / cas / str(i)
            pcap = d / "pair.pcap.gz"
            capture = ethernet(pcap)
            for sens in ("tx","rx"):
                exiger(capture["trames"].get(sens, 0) == serie["trames_par_sens"] and
                       capture["octets"].get(sens, 0) == serie["octets_par_sens"], f"PCAP {cas}/{i}")
            captures[f"{cas}/{i}"] = capture
            for p in sorted(d.iterdir()):
                if p.name in ("labo.tpm", "novaos.elf") or not p.is_file():
                    continue
                brut = p.name.startswith("qemu.trace") or p == pcap
                inclure = i == 1 or not brut
                relatif = str(p.relative_to(archive))
                inventaire[relatif] = dict(octets=p.stat().st_size, sha256=sha(p), inclus=inclure)
                if inclure:
                    membres.append(p)
    for cas in resultat["profils"]:
        exiger(len({captures[f"{cas}/{i}"]["sha256_sans_temps"] for i in (1,2,3)}) == 1,
               f"captures différentes entre démarrages de {cas}")
    exiger(len({captures[f"{cas}/1"]["sha256_sans_temps"] for cas in PERF}) == 1,
           "les témoins Ethernet diffèrent du raccord dynamique")
    membres.extend(sorted(archive.glob("images/*/construction.json")))
    for repertoire in ("hotes", "objets"):
        membres.extend(sorted((archive / repertoire).glob("*")))
    for p in membres:
        relatif = str(p.relative_to(archive))
        if relatif not in inventaire:
            inventaire[relatif] = dict(octets=p.stat().st_size, sha256=sha(p), inclus=True)
    write(sortie / "inventaire.json", inventaire)
    write(sortie / "captures.json", captures)
    with (sortie / "preuves.tar.gz").open("wb") as dest, gzip.GzipFile(fileobj=dest, filename="", mode="wb", mtime=0) as gz:
        with tarfile.open(fileobj=gz, mode="w|", format=tarfile.USTAR_FORMAT) as tar:
            for p in sorted(membres):
                info = tarfile.TarInfo(str(p.relative_to(archive)))
                info.size = p.stat().st_size
                with p.open("rb") as src:
                    tar.addfile(info, src)
    fichiers = ("bilan.json", "inventaire.json", "captures.json", "preuves.tar.gz")
    (sortie / "SHA256SUMS").write_text("".join(f"{sha(sortie / n)}  {n}\n" for n in fichiers))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("sortie", type=Path)
    args = parser.parse_args()
    capsule(args.archive, args.sortie)
