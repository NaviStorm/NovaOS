"""Consolide les traces originelles d'une campagne Ethernet v1 sans redémarrer QEMU."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import struct
import tarfile

SCENARIOS = "port boite data plein veille filtre court deces incertain revocation mesure60 mesure1514 endurance temoin16".split()
STATION = bytes.fromhex("525400123456")
PAIR = bytes.fromhex("024e56000001")


def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def capture(path, filtre=False):
    prefix = [hashlib.sha256(), hashlib.sha256()]
    counts, octets = [0, 0], [0, 0]
    rejects = [0, 0]
    with gzip.open(path, "rb") as f:
        if struct.unpack("<IHHIIII", f.read(24)) != (0xa1b2c3d4, 2, 4, 0, 0, 65535, 1):
            raise ValueError(f"PCAP inattendu dans {path}")
        while header := f.read(16):
            _, _, n, orig = struct.unpack("<IIII", header)
            frame = f.read(n)
            if n != orig or n not in (60, 1514) or len(frame) != n:
                raise ValueError(f"Trame tronquée ou hors profil dans {path}")
            if filtre and sum(rejects) < 16:
                i = sum(rejects) % 2
                src = bytes.fromhex("024e56000002") if i else PAIR
                typ = bytes.fromhex("88b5" if i else "88b6")
                if frame[:14] != STATION + src + typ:
                    raise ValueError(f"Séquence de filtrage incorrecte {path}")
                rejects[i] += 1
                continue
            if frame[12:14] != bytes.fromhex("88b5"):
                raise ValueError(f"EtherType inattendu {path}")
            sens = [PAIR + STATION, STATION + PAIR].index(frame[:12])
            if counts[sens] < 16:
                prefix[sens].update(struct.pack(">H", n) + frame)
            counts[sens] += 1
            octets[sens] += n
    return dict(trames=counts, octets=octets, rx_hors_profil=rejects, sha256_prefixe16=[p.hexdigest() for p in prefix])


def ajouter(tar, path, nom):
    donnees = path.read_bytes()
    info = tarfile.TarInfo(nom)
    info.size = len(donnees)
    tar.addfile(info, io.BytesIO(donnees))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("source", type=Path)
    p.add_argument("sortie", type=Path)
    args = p.parse_args()
    args.sortie.mkdir(parents=True, exist_ok=True)
    rows, inventaire = {}, {}
    selected = {"arguments.json", "campagne.json", "serie.log", "tables.json", "fin.json"}
    for cas in SCENARIOS:
        for isa in ("arm", "q35"):
            name = f"{cas}/{isa}"
            rep = args.source / name
            run = json.loads((rep / "campagne.json").read_text())
            if not run["succes"] or run["sources_suivies_modifiees"] or run["cas"] != cas or run["arch"] != isa:
                raise ValueError(name)
            hashes = {f.name: digest(f) for f in sorted(rep.iterdir()) if f.is_file()}
            if hashes["novaos.elf"] != run["elf"]:
                raise ValueError(f"ELF incohérent {name}")
            inventaire[name] = hashes
            cap = capture(rep / "pair.pcap.gz", cas == "filtre")
            if sum(cap["rx_hors_profil"]) != run.get("rx_filtrees", 0):
                raise ValueError(f"Compteur de filtrage {name}")
            tx, rx = cap["trames"]
            if tx != run["trames_par_sens"] or rx != tx + run["rx_sans_tx"] or cap["octets"][0] != run["octets_par_sens"]:
                raise ValueError(f"Compteurs différents {name}")
            lines = (rep / "serie.log").read_text(errors="replace").splitlines()
            s = "\n".join(lines)
            row = dict(campagne=run, capture=cap)
            if cas != "temoin16":
                for key, line in (("session", "ETH3 session "), ("bilan", "ETH3 bilan "), ("ipc", "ETH3 IPC ")):
                    row[key] = next(x for x in lines if x.startswith(line))
            if cas in ("mesure60", "mesure1514"):
                hz = int(re.search(r"\bhz=(\d+)", row["bilan"])[1])
                span = int(re.search(r"serie_ticks=(\d+)", row["bilan"])[1])
                row["mbit_s_tx"] = round(8 * cap["octets"][0] * hz / span / 1e6, 4)
                row["lots"] = []
                for b, n, total, qs in re.findall(r"ETH3 lot=(\d+) n=(\d+) somme_ticks=(\d+) quantiles_ticks=\[([0-9, ]+)\]", s):
                    taille, nb = int(b), int(n)
                    octets = int(cas.removeprefix("mesure"))
                    row["lots"].append(dict(taille=taille, n=nb,
                        latence_us=[round(int(t) * 1e6 / hz, 2) for t in qs.split(",")],
                        mbit_s_tx=round(taille * nb * octets * 8 * hz / int(total) / 1e6, 4)))
                if [(v["taille"], v["n"]) for v in row["lots"]] != [(b,128) for b in range(1,9)]:
                    raise ValueError(f"Cellules incomplètes {name}")
            rows[name] = row
    if len({r["campagne"]["commit_sources"] for r in rows.values()}) != 1:
        raise ValueError("Sources de campagne hétérogènes")
    for isa in ("arm", "q35"):
        if rows[f"temoin16/{isa}"]["capture"]["sha256_prefixe16"] != rows[f"port/{isa}"]["capture"]["sha256_prefixe16"]:
            raise ValueError(f"Équivalence du préfixe {isa}")
    if rows["port/arm"]["capture"]["sha256_prefixe16"] != rows["port/q35"]["capture"]["sha256_prefixe16"]:
        raise ValueError("Équivalence entre architectures")
    with (args.sortie / "preuves.tar.gz").open("wb") as out, gzip.GzipFile(filename="", fileobj=out, mode="wb", mtime=0) as gz, tarfile.open(fileobj=gz, mode="w|") as tar:
        for name, hashes in sorted(inventaire.items()):
            for f in hashes:
                if f in selected or (f == "pair.pcap.gz" and name.split("/")[0] in ("port", "veille", "filtre", "temoin16", "deces", "court")):
                    ajouter(tar, args.source / name / f, f"{name}/{f}")
    result = dict(serie=args.source.name, essais=rows, inventaire_sha256=inventaire,
                  preuves_sha256=digest(args.sortie / "preuves.tar.gz"))
    (args.sortie / "bilan.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
