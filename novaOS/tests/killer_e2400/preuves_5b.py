"""Rassemble les preuves 5B. Les seules images de disque incluses sont les deux volumes NovaFS fictifs Q35."""
import argparse
from collections import Counter
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import struct
import tarfile


def sha(data):
    return hashlib.sha256(data).hexdigest()


def document(data):
    return (json.dumps(data, indent=2, ensure_ascii=False) + "\n").encode()


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("hotes", "courtier", "contrat", "double", "arm", "sortie"):
        p.add_argument(name, type=Path)
    p.add_argument("--cibles", nargs=2, type=Path, required=True, metavar=("ARM", "X86"))
    a = p.parse_args()
    entries = {}
    traces = {}
    roots = Path(__file__).resolve().parents[4]
    def include(name, path, binary=False):
        original = path.read_bytes()
        content = original
        if not binary:
            for local, alias in ((roots, "$WORKTREES"), (Path.home(), "$HOME")):
                content = content.replace(str(local).encode(), alias.encode())
        entries[name] = content
        traces[name] = {"original": sha(original), "distribue": sha(content)}
        return sha(original)

    hosts = json.loads((a.hotes/"hotes.json").read_bytes())
    for isa, suites in hosts["suites"].items():
        require(suites["rosetta"] == (isa == "x86_64"), isa)
        for suite, count in (("e2400",44), ("gestionnaire",33), ("virtio-net",8), ("nvme",16), ("nvme-diagnostic",2)):
            name = f"{suite}-{isa}.log"
            require(suites[suite]["tests"] == count and suites[suite]["sha256"] == include("hotes/"+name, a.hotes/name), name)
    require(set(hosts["suites"]) == {"aarch64", "x86_64"} and hosts["abi"] == 48, "Deux architectures et ABI")
    for name, path in zip(("aarch64", "x86_64"), a.cibles):
        require("Finished `release` profile" in path.read_text(), name)
        include("cibles/"+name+".log", path)

    guest = json.loads((a.courtier/"banc.json").read_bytes())
    require(include("courtier/serie.log", a.courtier/"serie.log") == guest["journal"], "Journal 5B")
    elf = (a.courtier/"acteur/x86_64-unknown-none/release/courtier-5b").read_bytes()
    require(sha(elf) == guest["elf_sha256"] and len(elf) == guest["elf_octets"], "ELF 5B")
    offset, = struct.unpack_from("<Q", elf, 32)
    width, count = struct.unpack_from("<HH", elf, 54)
    segments = []
    for i in range(count):
        kind, flags, _, va, _, size, mem, _ = struct.unpack_from("<II6Q", elf, offset+i*width)
        if kind == 1:
            segments.append(dict(droits=flags, fichier=size, memoire=mem, pages=(va%4096+mem+4095)//4096))
    guest["segments"] = segments
    require(guest["tas_ring3"]["vivants_fin"] == 0 and "restitution=OK" in (a.courtier/"serie.log").read_text(), "Tas Ring 3")
    contract = json.loads((a.contrat/"banc.json").read_bytes())
    require(include("contrat/serie.log", a.contrat/"serie.log") == contract["journal"], "Journal 5A")
    serial = (a.contrat/"serie.log").read_text()
    require(serial.count("restitution=OK libres=61505") == 4 and serial.count("restitution_CPU=OK libres=61505") == 2, "Six sessions du témoin 5A")

    duo = {}
    for case, held in (("duo-retrait", 0), ("duo-absent", 26)):
        src = a.double/"double/nominal"/case/"1"
        report = json.loads((src/"campagne.json").read_bytes())
        files = ("campagne.json", "reprise.json", "serie.log", "qemu.trace", "pair.pcap.gz", "namespace.raw")
        for f in files:
            include("q35/"+case+"/"+f, src/f, f in ("pair.pcap.gz", "namespace.raw"))
        d = report["stockage"]
        ops = Counter(re.findall(r"^E3D CQE op=(READ|WRITE|FLUSH)", (src/"serie.log").read_text(), re.M))
        require(report["succes"] and d["p9"] and report["p9_final"] == sha((src/"namespace.raw").read_bytes())
                and d["cadres_nic_retenus"] == held and set(ops) == {"READ", "WRITE", "FLUSH"}, case)
        require(d["E_S_apres_WAIT_NIC"] > 0 and d["cycles"][0]["ordre"]["confirmation_apres_f2"], case)
        if held == 0:
            require(report["garde"]["cqe_retrouvee"], "Garde de réadmission Q35")
        else:
            require(report["nvme_actif"]["bme"] == {"48": True, "56": False}, "Indépendance du stockage Q35")
        duo[case] = dict(revision=report["commit_sources"], modele=report["modele"], elf=report["elf"],
                         p9_final=report["p9_final"], cadres_nic_retenus=held, trames_par_sens=report["trames_par_sens"],
                         cqe=dict(ops), io_apres_attente=d["E_S_apres_WAIT_NIC"], garde=report.get("garde"))

    arm = []
    for n in (1, 2, 3):
        src = a.arm/"ethernet"/str(n)
        report = json.loads((src/"campagne.json").read_bytes())
        serial = (src/"serie.log").read_text()
        boot, = re.findall(r"^E41 demarrage boot=([0-9a-f]{32})", serial, re.M)
        require(report["succes"] and report["elf_relu_esp"] == report["elf"] and report["volume_p9"]
                and report["trames_par_sens"] == 13312 and "creations=1 epingles=0 mappes=0" in serial, f"ARM {n}")
        for name in ("campagne.json", "serie.log"):
            include(f"arm/{n}/"+name, src/name)
        pcap = sha((src/"pair.pcap.gz").read_bytes())
        if n == 1:
            include("arm/1/pair.pcap.gz", src/"pair.pcap.gz", True)
        arm.append(dict(revision=report["commit_sources"], boot=boot, elf=report["elf"],
                        modele=report["modele"], firmware=report["firmware"], trames_par_sens=13312,
                        octets_par_sens=report["octets_par_sens"], capture_sha256=pcap))
    require(len({v["boot"] for v in arm}) == 3, "Époques ARM")
    a.sortie.mkdir(parents=True, exist_ok=False)
    entries["empreintes.json"] = document(traces)
    archive = a.sortie/"preuves-5b.tar.gz"
    with archive.open("xb") as f, gzip.GzipFile(fileobj=f, filename="", mode="wb", mtime=0) as gz:
        with tarfile.open(fileobj=gz, mode="w|") as tar:
            for name, data in sorted(entries.items()):
                info = tarfile.TarInfo(name)
                info.size = len(data)
                tar.addfile(info, io.BytesIO(data))
    for name, data in (("hotes-5b",hosts), ("q35-5b",guest),
                       ("compatibilite",dict(contrat=contract, q35=duo, arm=arm))):
        (a.sortie/(name+".json")).write_bytes(document(data))
    print(f"{len(entries)} pièces, archive SHA-256 {sha(archive.read_bytes())}")


if __name__ == "__main__":
    main()
