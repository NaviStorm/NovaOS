"""Emballage déterministe et contre-lecture hors invité des preuves 5F, bibliothèque standard."""

import argparse
import gzip
import io
import json
import re
import shutil
import struct
import tarfile
from hashlib import sha256
from pathlib import Path

NOVA = Path(__file__).resolve().parents[2]
PROOFS = NOVA / "docs/validation/diagnostic-uefi-5f"
NAMES = (
    "amovible",
    "sata",
    "ordre-usb",
    "ordre-sata",
    "prochain-usb",
    "prochain-absent",
    "demarrage-suivant",
    "prochain-usb-refuse",
    "support-absent",
    "sans-signature",
    "substitution",
    "configuration",
    "noyau",
    "ancienne",
    "db-tierce",
    "signataire-revoque",
    "signature-interne",
    "epoque",
)
EVIDENCE = (
    "firmware.log",
    "transactions.log",
    "cpu-final.txt",
    "qemu.log",
    "compteurs-qmp.jsonl",
    "rapport-ram.bin",
    "variables.fd",
    "pci-entree.json",
    "pci-sortie.json",
    "pci-fin.json",
    "vtd-entree.json",
    "vtd-sortie.json",
    "vtd-fin.json",
)


def require(condition, what):
    if not condition:
        raise ValueError(what)


def fingerprint(data):
    return sha256(data).hexdigest()


def pack(src, trust, dest):
    dest.mkdir(parents=True, exist_ok=False)
    shutil.copyfile(src / "bilan.json", dest / "bilan.json")
    shutil.copyfile(trust, dest / "confiance.json")
    with (
        (dest / "observations.tar.gz").open("xb") as disk,
        gzip.GzipFile(filename="", mode="wb", fileobj=disk, mtime=0) as zipfile,
        tarfile.open(fileobj=zipfile, mode="w|") as tar,
    ):
        for p in sorted(src.rglob("*")):
            if not p.is_file() or not (p.name in EVIDENCE or p.suffix == ".fd"):
                continue
            data = p.read_bytes()
            item = tarfile.TarInfo(p.relative_to(src).as_posix())
            item.size = len(data)
            tar.addfile(item, io.BytesIO(data))
    with (dest / "SHA256SUMS").open("x") as output:
        for name in ("bilan.json", "confiance.json", "observations.tar.gz"):
            output.write(f"{fingerprint((dest / name).read_bytes())}  {name}\n")


def check(folder):
    files = set()
    for line in (folder / "SHA256SUMS").read_text().splitlines():
        h, name = line.split()
        require(
            name in ("bilan.json", "confiance.json", "observations.tar.gz")
            and name not in files,
            name,
        )
        require(fingerprint((folder / name).read_bytes()) == h, name)
        files.add(name)
    require(len(files) == 3, "Sommes incomplètes")
    result = json.loads((folder / "bilan.json").read_bytes())
    trust = json.loads((folder / "confiance.json").read_bytes())
    require(
        len(trust["cas"]) == 8
        and sum(x["verifie"] for x in trust["cas"].values()) == 4,
        "Contre-épreuve PE",
    )
    old = trust["cas"]["ancienne"]["authenticode"]
    require(
        {"type": "empreinte-PE", "sha256": old} in trust["magasins"]["nominal"]["dbx"],
        "Ancienne génération",
    )
    require(
        {"type": "certificat", "sha256": trust["certificats"]["db"]}
        in trust["magasins"]["revoque"]["dbx"],
        "Signataire db",
    )
    require(
        set(result["cas"]) == {"direct", "pont"} and "liberation" in result["garde"],
        "Couverture/fin du verrou",
    )
    with tarfile.open(folder / "observations.tar.gz", "r:gz") as archive:

        def read(name):
            entry = archive.getmember(name)
            require(entry.isfile() and entry.size <= 2 * 1024 * 1024, name)
            return archive.extractfile(entry).read()

        for topo, cases in result["cas"].items():
            require(set(cases) == set(NAMES), topo)
            for name, run in cases.items():
                prefix = topo + "/" + name + "/"
                require(
                    run["avant"]["confiance"] == run["apres"]["confiance"]
                    and not run["apres"]["prochain"],
                    prefix + "variables",
                )
                if name == "db-tierce":
                    require(
                        fingerprint(read("db-tierce.fd")) == run["magasin"],
                        "Origine du magasin tiers",
                    )
                if name in ("prochain-usb", "prochain-usb-refuse", "support-absent"):
                    require(
                        run["avant"]["prochain"] == [0x51]
                        and run["avant"]["ordre"] == [0x50, 0x51],
                        prefix + "BootNext USB",
                    )
                if name == "prochain-absent":
                    require(
                        run["avant"]["prochain"] == [0x5F]
                        and "Boot005F" not in run["avant"]["options"],
                        prefix + "BootNext absent",
                    )
                if name == "demarrage-suivant":
                    require(
                        run["magasin"] == cases["prochain-usb"]["magasin_final"],
                        "Séquence BootNext",
                    )
                require(
                    fingerprint(read(prefix + "variables.fd")) == run["magasin_final"],
                    prefix + "VARS",
                )
                qmp = [
                    next(row["stats"] for row in rows if row.get("device") == "interne")
                    for rows in map(
                        json.loads, read(prefix + "compteurs-qmp.jsonl").splitlines()
                    )
                ]
                executed = name not in ("signataire-revoque", "db-tierce")
                require(len(qmp) == (3 if executed else 2), prefix + "QMP")
                for snap, summary in zip(
                    qmp,
                    (
                        [run["initial"], run["entree"], run["fin"]]
                        if executed
                        else [run["initial"], run["fin"]]
                    ),
                    strict=True,
                ):
                    require(
                        all(snap[k] == v for k, v in summary.items()),
                        prefix + "compteurs",
                    )
                require(
                    not any(run["initial"].values())
                    and run["disque_intact"]
                    and all(
                        v == 0 for k, v in run["fin"].items() if not k.startswith("rd_")
                    ),
                    prefix + "disque",
                )
                trace = read(prefix + "transactions.log")
                log = read(prefix + "firmware.log")
                require(
                    (b"Access Denied" in log or b"Security Violation" in log)
                    == run["refus_usb"],
                    prefix + "BDS",
                )
                require(len(trace) == run["octets_trace"], prefix + "taille trace")
                for phase in ("entree", "sortie", "fin") if executed else ("fin",):
                    pci = json.loads(read(prefix + "pci-" + phase + ".json"))
                    units = json.loads(read(prefix + "vtd-" + phase + ".json"))
                    require(
                        pci == run["pci"][phase] and units == run["vtd"][phase],
                        prefix + phase,
                    )
                    require(
                        len(units) == int(topo == "pont")
                        and not any(x["tes"] for x in units),
                        prefix + "TES",
                    )
                    for dev in pci["controleurs"].values():
                        require(
                            int(dev["lecture"].split(":")[1], 16) == dev["commande"]
                            and dev["bme"] == bool(dev["commande"] & 4),
                            prefix + "PCI",
                        )
                if executed:
                    ram = read(prefix + "rapport-ram.bin")
                    (
                        addr,
                        entry,
                        stop,
                        base,
                        status,
                        tables,
                        dmar,
                        drhd,
                        nodes,
                        arena,
                        live,
                        exit_addr,
                    ) = struct.unpack_from("<12Q", ram, 16)
                    expected = {
                        "amovible": 0,
                        "ordre-usb": 0,
                        "prochain-usb": 0,
                        "signature-interne": 6,
                        "epoque": 12,
                    }.get(name, 9)
                    require(
                        status == expected and (status == 0 or nodes == 0),
                        prefix + "statut et publication",
                    )
                    r = run["rapport"]
                    require(
                        (base, tables, dmar, drhd, nodes, arena, live)
                        == (
                            r["base"],
                            r["tables"],
                            r["dmar"],
                            r["drhd"],
                            len(r["noeuds"]),
                            r["arena"],
                            r["vivants"],
                        ),
                        prefix + "rapport RAM",
                    )
                    require(
                        ram.startswith(b"NOVA5E-RAM-FINAL")
                        and (addr, entry, stop, status, exit_addr)
                        == (
                            r["adresse"],
                            r["entree"],
                            r["arret"],
                            r["statut"],
                            r["sortie_efi"],
                        ),
                        prefix + "RAM",
                    )
                    require(
                        run["pci"]["entree"]["rip"] == entry
                        and run["pci"]["sortie"]["rip"] == exit_addr,
                        prefix + "RIP",
                    )
                    require(
                        all(
                            x["bme"]
                            for x in run["pci"]["entree"]["controleurs"].values()
                        ),
                        prefix + "BME",
                    )
                    require(
                        qmp[1]["rd_operations"] == qmp[2]["rd_operations"]
                        and run["entree"] == run["fin"],
                        prefix + "intervalle",
                    )
                    require(
                        run["octets_avant"] == run["octets_avant_sortie"]
                        and trace[run["octets_avant"] :].decode().splitlines()
                        == run["evenements_diagnostic"],
                        prefix + "trace",
                    )
                    for event in run["evenements_diagnostic"]:
                        match = re.fullmatch(
                            r"pci_cfg_write (qemu-xhci \S+) @0x4 <- (0x[0-9a-f]+)",
                            event,
                        )
                        require(
                            match
                            and run["commandes_pci_entree"].get(match[1])
                            == int(match[2], 16),
                            prefix + "callback",
                        )
                    regs = read(prefix + "cpu-final.txt")
                    rip = int(re.search(rb"RIP=([0-9a-fA-F]+)", regs)[1], 16)
                    flags = int(re.search(rb"RFL=([0-9a-fA-F]+)", regs)[1], 16)
                    require(
                        b"HLT=1" in regs
                        and not flags & 0x200
                        and stop <= rip < stop + 8
                        and live == 0,
                        prefix + "arrêt",
                    )
                else:
                    require(
                        b"No bootable option" in log and run["rapport"] is None,
                        prefix + "rejet terminal",
                    )
                # Mesures de référence du firmware livré, jamais un quota d'E/S autorisé.
                counts = {
                    "amovible": (0, 0),
                    "signature-interne": (0, 0),
                    "epoque": (0, 0),
                    "sata": (7, 345088),
                    "ordre-usb": (6, 4608),
                    "prochain-usb": (6, 4608),
                }
                require(
                    (qmp[-1]["rd_operations"], qmp[-1]["rd_bytes"])
                    == counts.get(name, (11, 348672)),
                    prefix + "mesure de référence",
                )
    print("36 cellules OVMF, 32 intervalles EFI et 8 contrôles Authenticode relus.")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--emballer", nargs=2, type=Path, metavar=("SERIE", "CONFIANCE"))
    p.add_argument("--dossier", type=Path, default=PROOFS)
    args = p.parse_args()
    if args.emballer:
        pack(*args.emballer, args.dossier)
    check(args.dossier)
